use std::collections::{HashMap, HashSet};
use std::fs;
use std::process::Command;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use tauri::{AppHandle, Manager};

const POLL: Duration = Duration::from_secs(5);
const SETTLE: Duration = Duration::from_secs(2);
const COOLDOWN: Duration = Duration::from_secs(30);
const ROUNDS_BEFORE_ESCALATION: u32 = 3;
const ESCALATIONS_PER_WINDOW: usize = 3;
const ESCALATION_WINDOW: Duration = Duration::from_secs(600);

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Direction {
    Capture,
    Playback,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Wedge {
    /// Card came back from a usb drop and pipewire re-opened it into a node
    /// that holds RUNNING but never delivers a frame.
    ZombieCapture,
    StuckXrun,
}

#[derive(Clone, PartialEq, Debug)]
pub struct Sample {
    pub state: String,
    pub tstamp_zero: bool,
    pub avail_max: u64,
    pub hw_ptr: u64,
}

#[derive(Clone, Debug)]
struct Pcm {
    card: u32,
    direction: Direction,
    sample: Sample,
}

pub fn spawn(app: AppHandle) {
    thread::spawn(move || {
        let mut prev: HashMap<String, Pcm> = HashMap::new();
        let mut rounds: HashMap<(u32, Direction), u32> = HashMap::new();
        let mut last_attempt: HashMap<(u32, Direction), Instant> = HashMap::new();
        let mut escalations: Vec<Instant> = Vec::new();
        loop {
            let now = scan();
            let wedged = confirm_all(&now, &prev);
            prev = now;
            if !wedged.is_empty() {
                recover(
                    &app,
                    &wedged,
                    &mut rounds,
                    &mut last_attempt,
                    &mut escalations,
                );
            } else {
                rounds.clear();
            }
            thread::sleep(POLL);
        }
    });
}

/// Parse one `/proc/asound/cardN/pcmNc/subN/status`. Keys carry ragged
/// whitespace before the colon, so split on the first one.
pub fn parse_status(text: &str) -> Option<Sample> {
    let mut fields: HashMap<&str, &str> = HashMap::new();
    for line in text.lines() {
        if let Some((key, value)) = line.split_once(':') {
            fields.insert(key.trim(), value.trim());
        }
    }
    let state = (*fields.get("state")?).to_string();
    let num = |key: &str| -> u64 {
        fields
            .get(key)
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(0)
    };
    Some(Sample {
        tstamp_zero: fields
            .get("tstamp")
            .map(|v| v.parse::<f64>().unwrap_or(0.0) == 0.0)
            .unwrap_or(true),
        avail_max: num("avail_max"),
        hw_ptr: num("hw_ptr"),
        state,
    })
}

fn bad_sample(s: &Sample) -> Option<Wedge> {
    if s.state == "RUNNING" && s.tstamp_zero && s.avail_max == 0 {
        return Some(Wedge::ZombieCapture);
    }
    if s.state == "XRUN" {
        return Some(Wedge::StuckXrun);
    }
    None
}

/// A wedge only counts once it survives two polls. One bad sample is normal
/// churn on startup and on device switches.
pub fn confirm(cur: &Sample, prev: Option<&Sample>) -> Option<Wedge> {
    let kind = bad_sample(cur)?;
    let prev = prev?;
    if bad_sample(prev)? != kind {
        return None;
    }
    match kind {
        Wedge::StuckXrun if prev.hw_ptr != cur.hw_ptr => None,
        _ => Some(kind),
    }
}

fn confirm_all(
    now: &HashMap<String, Pcm>,
    prev: &HashMap<String, Pcm>,
) -> HashSet<(u32, Direction)> {
    let mut out = HashSet::new();
    for (path, pcm) in now {
        let before = prev.get(path).map(|p| &p.sample);
        if confirm(&pcm.sample, before).is_some() {
            out.insert((pcm.card, pcm.direction));
        }
    }
    out
}

fn scan() -> HashMap<String, Pcm> {
    let mut out = HashMap::new();
    let cards = match fs::read_dir("/proc/asound") {
        Ok(d) => d,
        Err(_) => return out,
    };
    for card in cards.flatten() {
        let card_name = card.file_name().to_string_lossy().into_owned();
        let number = match card_name
            .strip_prefix("card")
            .and_then(|n| n.parse::<u32>().ok())
        {
            Some(n) => n,
            None => continue,
        };
        let pcms = match fs::read_dir(card.path()) {
            Ok(d) => d,
            Err(_) => continue,
        };
        for pcm in pcms.flatten() {
            let pcm_name = pcm.file_name().to_string_lossy().into_owned();
            if !pcm_name.starts_with("pcm") {
                continue;
            }
            let direction = match pcm_name.chars().last() {
                Some('c') => Direction::Capture,
                Some('p') => Direction::Playback,
                _ => continue,
            };
            let subs = match fs::read_dir(pcm.path()) {
                Ok(d) => d,
                Err(_) => continue,
            };
            for sub in subs.flatten() {
                let status = sub.path().join("status");
                let text = match fs::read_to_string(&status) {
                    Ok(t) => t,
                    Err(_) => continue,
                };
                if let Some(sample) = parse_status(&text) {
                    out.insert(
                        status.to_string_lossy().into_owned(),
                        Pcm {
                            card: number,
                            direction,
                            sample,
                        },
                    );
                }
            }
        }
    }
    out
}

fn recover(
    app: &AppHandle,
    wedged: &HashSet<(u32, Direction)>,
    rounds: &mut HashMap<(u32, Direction), u32>,
    last_attempt: &mut HashMap<(u32, Direction), Instant>,
    escalations: &mut Vec<Instant>,
) {
    let due: Vec<(u32, Direction)> = wedged
        .iter()
        .copied()
        .filter(|key| {
            last_attempt
                .get(key)
                .map(|t| t.elapsed() >= COOLDOWN)
                .unwrap_or(true)
        })
        .collect();
    if due.is_empty() {
        return;
    }
    for key in &due {
        last_attempt.insert(*key, Instant::now());
        let count = rounds.entry(*key).or_insert(0);
        *count += 1;
    }

    // Sources first. When the Wave XLR wedged on 2026-08-17 suspending the two
    // xrun sinks changed nothing, and suspending the dead capture freed all of
    // them. The sinks are downstream victims, so clearing them first is wasted.
    let capture_cards: HashSet<u32> = due
        .iter()
        .filter(|(_, d)| *d == Direction::Capture)
        .map(|(c, _)| *c)
        .collect();
    for (card, name) in nodes("sources", &capture_cards) {
        eprintln!("audio watchdog: suspending wedged source on card {card}: {name}");
        suspend_cycle("suspend-source", &name);
    }

    let (before, after) = scan_pair();
    let still = confirm_all(&after, &before);
    let playback_cards: HashSet<u32> = due
        .iter()
        .filter(|(_, d)| *d == Direction::Playback)
        .map(|(c, _)| *c)
        .filter(|c| still.contains(&(*c, Direction::Playback)))
        .collect();
    for (card, name) in nodes("sinks", &playback_cards) {
        eprintln!("audio watchdog: suspending wedged sink on card {card}: {name}");
        suspend_cycle("suspend-sink", &name);
    }

    let (before, after) = scan_pair();
    let remaining = confirm_all(&after, &before);
    for key in &due {
        if !remaining.contains(key) {
            rounds.remove(key);
            eprintln!("audio watchdog: card {} recovered", key.0);
        }
    }
    let stuck = due.iter().any(|k| {
        remaining.contains(k) && rounds.get(k).copied().unwrap_or(0) >= ROUNDS_BEFORE_ESCALATION
    });
    if stuck {
        escalate(app, rounds, escalations);
    }
}

/// Two readings a second apart, so `confirm` compares against a fresh previous
/// sample instead of the pre-recovery one. A PCM that just resumed needs a
/// cycle before its numbers mean anything.
fn scan_pair() -> (HashMap<String, Pcm>, HashMap<String, Pcm>) {
    let before = scan();
    thread::sleep(Duration::from_secs(1));
    (before, scan())
}

fn suspend_cycle(verb: &str, name: &str) {
    let _ = Command::new("pactl").args([verb, name, "1"]).status();
    thread::sleep(SETTLE);
    let _ = Command::new("pactl").args([verb, name, "0"]).status();
}

/// Map wedged card numbers to pipewire node names. Monitor sources are skipped:
/// they mirror a sink and are never the stuck device.
fn nodes(kind: &str, cards: &HashSet<u32>) -> Vec<(u32, String)> {
    if cards.is_empty() {
        return Vec::new();
    }
    let json = Command::new("pactl")
        .args(["-f", "json", "list", kind])
        .output()
        .ok()
        .and_then(|o| serde_json::from_slice::<serde_json::Value>(&o.stdout).ok())
        .unwrap_or_default();
    let mut out = Vec::new();
    for node in json.as_array().unwrap_or(&Vec::new()) {
        let name = match node["name"].as_str() {
            Some(n) if !n.ends_with(".monitor") => n,
            _ => continue,
        };
        let card = node["properties"]["alsa.card"]
            .as_str()
            .and_then(|v| v.parse::<u32>().ok())
            .or_else(|| node["properties"]["alsa.card"].as_u64().map(|v| v as u32));
        if let Some(card) = card {
            if cards.contains(&card) {
                out.push((card, name.to_string()));
            }
        }
    }
    out
}

/// Last resort. `restart_pipewire_stack` wipes source mutes and severs the LV2
/// host links, so it stays behind the round counter and the window cap.
fn escalate(
    app: &AppHandle,
    rounds: &mut HashMap<(u32, Direction), u32>,
    escalations: &mut Vec<Instant>,
) {
    escalations.retain(|t| t.elapsed() < ESCALATION_WINDOW);
    if escalations.len() >= ESCALATIONS_PER_WINDOW {
        eprintln!(
            "audio watchdog: still wedged after {ESCALATIONS_PER_WINDOW} restarts, backing off"
        );
        return;
    }
    let registry = match app.try_state::<Arc<tideline_host::PluginRegistry>>() {
        Some(r) => r.inner().clone(),
        None => {
            eprintln!("audio watchdog: plugin registry not ready, skipping restart");
            return;
        }
    };
    escalations.push(Instant::now());
    rounds.clear();
    eprintln!("audio watchdog: suspend/resume did not take, restarting the pipewire stack");
    tauri::async_runtime::block_on(crate::restart_pipewire_stack(&registry));
}

#[cfg(test)]
mod tests {
    use super::*;

    const ZOMBIE: &str = "state: RUNNING\nowner_pid   : 1009919\ntrigger_time: 755167.072147047\ntstamp      : 0.000000000\ndelay       : 0\navail       : 0\navail_max   : 0\n";
    const XRUN: &str = "state: XRUN\nowner_pid   : 1009919\ntrigger_time: 755186.232526113\ntstamp      : 755387.788096377\ndelay       : 0\navail       : 32544\navail_max   : 32544\n-----\nhw_ptr      : 203808\nappl_ptr    : 204032\n";
    const HEALTHY: &str = "state: RUNNING\nowner_pid   : 1009919\ntrigger_time: 755167.072147047\ntstamp      : 755387.788096377\ndelay       : 0\navail       : 216\navail_max   : 432\n-----\nhw_ptr      : 900000\nappl_ptr    : 900128\n";

    fn sample(text: &str) -> Sample {
        parse_status(text).expect("parses")
    }

    #[test]
    fn parses_the_zombie_capture() {
        let s = sample(ZOMBIE);
        assert_eq!(s.state, "RUNNING");
        assert!(s.tstamp_zero);
        assert_eq!(s.avail_max, 0);
    }

    #[test]
    fn parses_hw_ptr_past_the_separator() {
        assert_eq!(sample(XRUN).hw_ptr, 203808);
    }

    #[test]
    fn healthy_is_never_a_wedge() {
        let s = sample(HEALTHY);
        assert_eq!(confirm(&s, Some(&s)), None);
    }

    #[test]
    fn one_bad_sample_does_not_trigger() {
        assert_eq!(confirm(&sample(ZOMBIE), None), None);
        assert_eq!(confirm(&sample(ZOMBIE), Some(&sample(HEALTHY))), None);
    }

    #[test]
    fn two_bad_samples_trigger() {
        assert_eq!(
            confirm(&sample(ZOMBIE), Some(&sample(ZOMBIE))),
            Some(Wedge::ZombieCapture)
        );
    }

    #[test]
    fn xrun_that_is_still_moving_is_not_wedged() {
        let mut moved = sample(XRUN);
        moved.hw_ptr += 1024;
        assert_eq!(confirm(&moved, Some(&sample(XRUN))), None);
    }

    #[test]
    fn xrun_with_a_frozen_pointer_is_wedged() {
        assert_eq!(
            confirm(&sample(XRUN), Some(&sample(XRUN))),
            Some(Wedge::StuckXrun)
        );
    }

    #[test]
    fn a_card_reports_both_directions_separately() {
        let now = HashMap::from([
            (
                "/proc/asound/card0/pcm0c/sub0/status".to_string(),
                Pcm {
                    card: 0,
                    direction: Direction::Capture,
                    sample: sample(ZOMBIE),
                },
            ),
            (
                "/proc/asound/card3/pcm0p/sub0/status".to_string(),
                Pcm {
                    card: 3,
                    direction: Direction::Playback,
                    sample: sample(XRUN),
                },
            ),
        ]);
        let wedged = confirm_all(&now, &now);
        assert!(wedged.contains(&(0, Direction::Capture)));
        assert!(wedged.contains(&(3, Direction::Playback)));
    }
}
