#[cfg(target_os = "linux")]
mod glib_log;
mod levels;
mod plugins;
mod routing;

use levels::LevelMonitor;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;
use std::time::Duration;
use tauri::{
    menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Manager, State, Wry,
};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

const TIDELINE_TRAY_ID: &str = "tideline-tray";

#[cfg(debug_assertions)]
const APP_TITLE: &str = "Tideline - Dev";
#[cfg(not(debug_assertions))]
const APP_TITLE: &str = "Tideline";

const MIX_ENABLED_FILE: &str = "/tmp/tideline-mix-enabled.json";
const VOLUMES_FILE: &str = "/tmp/tideline-volumes.json";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SinkInput {
    pub index: u32,
    pub node_name: String,
    pub muted: bool,
    pub volume: u32,
}

pub use tideline_core::model::{AppConfig, ChannelCfg, ChannelKind, KeybindAction, Mix};

#[derive(Debug, Clone, Serialize)]
pub struct SinkInfo {
    pub name: String,
    pub description: String,
    pub muted: bool,
    pub volume_percent: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct SourceInfo {
    pub name: String,
    pub description: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct RunningApp {
    pub binary: String,
    pub application_name: String,
    pub sink: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct CardControl {
    pub name: String,
    pub volume_percent: u32,
    pub muted: bool,
    pub has_volume: bool,
    pub has_switch: bool,
    pub is_capture: bool,
    pub is_playback: bool,
    pub current_db: Option<f32>,
}

pub struct AppState {
    pub mix_enabled: Mutex<HashMap<String, bool>>,
    pub config: Mutex<AppConfig>,
}

use tideline_core::config_io::{load_config, save_config_to_disk, slug};

fn pactl(args: &[&str]) {
    let _ = Command::new("pactl").args(args).status();
}

fn pactl_output(args: &[&str]) -> String {
    Command::new("pactl")
        .args(args)
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
        .unwrap_or_default()
}

#[derive(Debug, Clone, Serialize)]
pub struct AudioBackendStatus {
    pub server_name: Option<String>,
    pub on_pipewire: bool,
    pub started_services: Vec<String>,
    pub errors: Vec<String>,
    pub affected_apps: Vec<String>,
}

static INITIAL_BACKEND: OnceLock<AudioBackendStatus> = OnceLock::new();

const KNOWN_AUDIO_APPS: &[(&str, &str)] = &[
    ("chrome", "Chrome"),
    ("google-chrome-stable", "Chrome"),
    ("google-chrome", "Chrome"),
    ("chromium", "Chromium"),
    ("chromium-browser", "Chromium"),
    ("brave", "Brave"),
    ("brave-browser", "Brave"),
    ("firefox", "Firefox"),
    ("firefox-esr", "Firefox"),
    ("Discord", "Discord"),
    ("discord", "Discord"),
    ("Slack", "Slack"),
    ("slack", "Slack"),
    ("spotify", "Spotify"),
    ("Spotify", "Spotify"),
    ("zoom", "Zoom"),
    ("teams", "Teams"),
    ("teams-for-linux", "Teams"),
    ("vlc", "VLC"),
    ("mpv", "mpv"),
    ("steam", "Steam"),
    ("obs", "OBS Studio"),
    ("audacity", "Audacity"),
    ("Telegram", "Telegram"),
    ("telegram-desktop", "Telegram"),
    ("element-desktop", "Element"),
    ("signal-desktop", "Signal"),
];

fn detect_running_audio_apps() -> Vec<String> {
    let mut seen: HashSet<&'static str> = HashSet::new();
    let mut out: Vec<String> = Vec::new();
    for (binary, label) in KNOWN_AUDIO_APPS {
        if seen.contains(label) {
            continue;
        }
        let running = Command::new("pgrep")
            .args(["-x", binary])
            .output()
            .map(|o| o.status.success() && !o.stdout.is_empty())
            .unwrap_or(false);
        if running {
            seen.insert(label);
            out.push((*label).to_string());
        }
    }
    out
}

fn pulse_server_name() -> Option<String> {
    let out = Command::new("pactl").args(["info"]).output().ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&out.stdout);
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("Server Name:") {
            return Some(rest.trim().to_string());
        }
    }
    None
}

fn systemctl_user_is_active(unit: &str) -> bool {
    Command::new("systemctl")
        .args(["--user", "is-active", "--quiet", unit])
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn systemctl_user_unit_exists(unit: &str) -> bool {
    let out = Command::new("systemctl")
        .args(["--user", "list-unit-files", "--no-legend", unit])
        .output();
    match out {
        Ok(o) => o.status.success() && !o.stdout.is_empty(),
        Err(_) => false,
    }
}

fn systemctl_user_start(unit: &str) -> Result<(), String> {
    let out = Command::new("systemctl")
        .args(["--user", "start", unit])
        .output()
        .map_err(|e| format!("spawn systemctl failed: {}", e))?;
    if out.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
        Err(if stderr.is_empty() {
            format!("systemctl --user start {} failed", unit)
        } else {
            stderr
        })
    }
}

fn ensure_audio_backend() -> AudioBackendStatus {
    let mut status = AudioBackendStatus {
        server_name: pulse_server_name(),
        on_pipewire: false,
        started_services: Vec::new(),
        errors: Vec::new(),
        affected_apps: Vec::new(),
    };

    if let Some(name) = &status.server_name {
        status.on_pipewire = name.contains("PipeWire");
        if status.on_pipewire || name.contains("pulseaudio") || name.contains("PulseAudio") {
            return status;
        }
    }

    let units = ["pipewire.socket", "pipewire-pulse.socket", "wireplumber.service"];
    for unit in units {
        if systemctl_user_is_active(unit) {
            continue;
        }
        if !systemctl_user_unit_exists(unit) {
            status.errors.push(format!("{} not installed", unit));
            continue;
        }
        match systemctl_user_start(unit) {
            Ok(()) => status.started_services.push(unit.to_string()),
            Err(e) => status.errors.push(format!("start {}: {}", unit, e)),
        }
    }

    if !status.started_services.is_empty() {
        status.affected_apps = detect_running_audio_apps();
    }

    for _ in 0..10 {
        thread::sleep(Duration::from_millis(200));
        if let Some(name) = pulse_server_name() {
            status.server_name = Some(name.clone());
            status.on_pipewire = name.contains("PipeWire");
            return status;
        }
    }

    if status.server_name.is_none() {
        status.errors.push("pactl info still returns no server after start attempt".to_string());
    }
    status
}

#[tauri::command]
fn ensure_audio_backend_cmd() -> AudioBackendStatus {
    ensure_audio_backend()
}

#[tauri::command]
fn get_initial_backend_status() -> Option<AudioBackendStatus> {
    INITIAL_BACKEND.get().cloned()
}

fn fetch_sink_inputs() -> Vec<SinkInput> {
    let raw = pactl_output(&["-f", "json", "list", "sink-inputs"]);
    let json: serde_json::Value = serde_json::from_str(&raw).unwrap_or_default();

    json.as_array()
        .unwrap_or(&vec![])
        .iter()
        .filter_map(|item| {
            let index = item["index"].as_u64()? as u32;
            let node_name = item["properties"]["node.name"].as_str()?.to_string();
            if !node_name.starts_with("playback.") {
                return None;
            }
            let muted = item["mute"].as_bool().unwrap_or(false);
            let volume = item["volume"]
                .as_object()
                .and_then(|m| m.values().next())
                .and_then(|v| v["value_percent"].as_str())
                .and_then(|s| s.trim_end_matches('%').parse::<u32>().ok())
                .unwrap_or(100);
            Some(SinkInput { index, node_name, muted, volume })
        })
        .collect()
}

fn fetch_sinks() -> Vec<SinkInfo> {
    let raw = pactl_output(&["-f", "json", "list", "sinks"]);
    let json: serde_json::Value = serde_json::from_str(&raw).unwrap_or_default();

    json.as_array()
        .unwrap_or(&vec![])
        .iter()
        .filter_map(|item| {
            let name = item["name"].as_str()?.to_string();
            if name.starts_with("sink.") {
                return None;
            }
            let description = item["description"]
                .as_str()
                .unwrap_or(&name)
                .to_string();
            let muted = item["mute"].as_bool().unwrap_or(false);
            let volume_percent = item["volume"]
                .as_object()
                .and_then(|m| m.values().next())
                .and_then(|v| v["value_percent"].as_str())
                .and_then(|s| s.trim_end_matches('%').parse::<u32>().ok())
                .unwrap_or(100);
            Some(SinkInfo { name, description, muted, volume_percent })
        })
        .collect()
}

fn fetch_sources() -> Vec<SourceInfo> {
    let raw = pactl_output(&["-f", "json", "list", "sources"]);
    let json: serde_json::Value = serde_json::from_str(&raw).unwrap_or_default();

    json.as_array()
        .unwrap_or(&vec![])
        .iter()
        .filter_map(|item| {
            let name = item["name"].as_str()?.to_string();
            let description = item["description"]
                .as_str()
                .unwrap_or(&name)
                .to_string();
            Some(SourceInfo { name, description })
        })
        .collect()
}

fn amixer_output(args: &[&str]) -> String {
    Command::new("amixer")
        .args(args)
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
        .unwrap_or_default()
}

fn parse_amixer_scontents(output: &str) -> Vec<CardControl> {
    let mut out: Vec<CardControl> = Vec::new();
    let mut current: Option<CardControl> = None;
    for raw in output.lines() {
        let line = raw.trim();
        if let Some(rest) = line.strip_prefix("Simple mixer control '") {
            if let Some(c) = current.take() {
                out.push(c);
            }
            if let Some(end) = rest.rfind("',") {
                let name = &rest[..end];
                current = Some(CardControl {
                    name: name.to_string(),
                    volume_percent: 0,
                    muted: false,
                    has_volume: false,
                    has_switch: false,
                    is_capture: false,
                    is_playback: false,
                    current_db: None,
                });
            }
        } else if let Some(c) = current.as_mut() {
            if let Some(caps) = line.strip_prefix("Capabilities:") {
                c.has_volume = caps.contains("volume");
                c.has_switch = caps.contains("switch");
                c.is_capture = caps.contains("cvolume") || caps.contains("cswitch");
                c.is_playback = caps.contains("pvolume") || caps.contains("pswitch");
            } else if line.contains('%') {
                if let Some(start) = line.find('[') {
                    if let Some(pe) = line[start..].find('%') {
                        if let Ok(v) = line[start + 1..start + pe].parse::<u32>() {
                            c.volume_percent = v;
                        }
                    }
                }
                if let Some(db_start) = line.find("dB]") {
                    if let Some(open) = line[..db_start].rfind('[') {
                        let inner = &line[open + 1..db_start];
                        if let Ok(db) = inner.parse::<f32>() {
                            c.current_db = Some(db);
                        }
                    }
                }
                if line.contains("[off]") {
                    c.muted = true;
                }
            }
        }
    }
    if let Some(c) = current.take() {
        out.push(c);
    }
    out
}

fn fetch_running_apps() -> Vec<RunningApp> {
    let raw = pactl_output(&["-f", "json", "list", "sink-inputs"]);
    let json: serde_json::Value = serde_json::from_str(&raw).unwrap_or_default();

    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut out = Vec::new();
    for item in json.as_array().unwrap_or(&vec![]) {
        let props = &item["properties"];
        let node_name = props["node.name"].as_str().unwrap_or("");
        if node_name.starts_with("playback.") || node_name.starts_with("capture.") {
            continue;
        }
        let binary = props["application.process.binary"].as_str().unwrap_or("").to_string();
        if binary.is_empty() { continue; }
        if !seen.insert(binary.clone()) { continue; }
        let application_name = props["application.name"].as_str().unwrap_or(&binary).to_string();
        let sink = item["sink"].as_u64().map(|n| n.to_string()).unwrap_or_default();
        out.push(RunningApp { binary, application_name, sink });
    }
    out
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelVolumes {
    #[serde(default = "default_master_pct")]
    pub master: u32,
    #[serde(default)]
    pub mixes: HashMap<String, u32>,
}

fn default_master_pct() -> u32 { 100 }

impl Default for ChannelVolumes {
    fn default() -> Self {
        Self { master: 100, mixes: HashMap::new() }
    }
}

fn read_all_volumes() -> HashMap<String, ChannelVolumes> {
    fs::read_to_string(VOLUMES_FILE)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn write_all_volumes(map: &HashMap<String, ChannelVolumes>) {
    if let Ok(s) = serde_json::to_string_pretty(map) {
        let _ = fs::write(VOLUMES_FILE, s);
    }
}

fn product_pct(master: u32, mix: u32) -> u32 {
    ((master.min(100) as u64 * mix.min(100) as u64) / 100) as u32
}

fn sink_input_indexes_for_channel_mix(channel_slug: &str, mix_id: &str) -> Vec<u32> {
    let inputs = fetch_sink_inputs();
    let prefix = format!("playback.{}-{}-", channel_slug, mix_id);
    inputs
        .iter()
        .filter(|i| i.node_name.starts_with(&prefix))
        .map(|i| i.index)
        .collect()
}

fn apply_channel_volumes(channel_slug: &str, vols: &ChannelVolumes, mix_ids: &[String]) {
    for mix_id in mix_ids {
        let mix_pct = vols.mixes.get(mix_id).copied().unwrap_or(100);
        let final_pct = product_pct(vols.master, mix_pct);
        for idx in sink_input_indexes_for_channel_mix(channel_slug, mix_id) {
            pactl(&["set-sink-input-volume", &idx.to_string(), &format!("{}%", final_pct)]);
        }
    }
}

#[tauri::command]
fn get_all_channel_volumes() -> HashMap<String, ChannelVolumes> {
    read_all_volumes()
}

#[tauri::command]
fn set_channel_master_volume(channel: String, pct: u32, state: State<'_, AppState>) {
    let cfg = state.config.lock().unwrap().clone();
    let mix_ids: Vec<String> = cfg.mixes.iter().map(|m| m.id.clone()).collect();
    let mut all = read_all_volumes();
    let entry = all.entry(channel.clone()).or_default();
    entry.master = pct.min(100);
    let snap = entry.clone();
    write_all_volumes(&all);
    apply_channel_volumes(&slug(&channel), &snap, &mix_ids);
}

#[tauri::command]
fn set_channel_mix_volume(channel: String, mix_id: String, pct: u32) {
    let mut all = read_all_volumes();
    let entry = all.entry(channel.clone()).or_default();
    entry.mixes.insert(mix_id.clone(), pct.min(100));
    let snap = entry.clone();
    write_all_volumes(&all);
    let final_pct = product_pct(snap.master, pct.min(100));
    for idx in sink_input_indexes_for_channel_mix(&slug(&channel), &mix_id) {
        pactl(&["set-sink-input-volume", &idx.to_string(), &format!("{}%", final_pct)]);
    }
}

fn read_mix_enabled() -> HashMap<String, bool> {
    fs::read_to_string(MIX_ENABLED_FILE)
        .ok()
        .and_then(|s| serde_json::from_str::<HashMap<String, bool>>(&s).ok())
        .unwrap_or_default()
}

fn write_mix_enabled(map: &HashMap<String, bool>) {
    if let Ok(s) = serde_json::to_string(map) {
        let _ = fs::write(MIX_ENABLED_FILE, s);
    }
}

fn mix_enabled_with_defaults(stored: &HashMap<String, bool>, mixes: &[Mix]) -> HashMap<String, bool> {
    mixes.iter()
        .map(|m| (m.id.clone(), *stored.get(&m.id).unwrap_or(&true)))
        .collect()
}

fn apply_mix_enabled(enabled: &HashMap<String, bool>, cfg: &AppConfig) {
    let inputs = fetch_sink_inputs();
    let lookup: HashMap<&str, u32> = inputs
        .iter()
        .map(|i| (i.node_name.as_str(), i.index))
        .collect();

    for ch in &cfg.channels {
        if ch.kind != ChannelKind::Output { continue; }
        for mix in &cfg.mixes {
            let mute_arg = if *enabled.get(&mix.id).unwrap_or(&true) { "0" } else { "1" };
            for (i, _) in mix.sinks.iter().enumerate() {
                let pb = mix_playback_node(ch, mix, i);
                if let Some(&idx) = lookup.get(pb.as_str()) {
                    pactl(&["set-sink-input-mute", &idx.to_string(), mute_arg]);
                }
            }
        }
    }
}

use tideline_core::pipewire::{
    mix_capture_node, mix_playback_node, sink_node_for_channel, write_app_routing,
    write_pipewire_conf_with_contributions,
};

fn restart_pipewire_stack() {
    let _ = Command::new("systemctl")
        .args(["--user", "restart", "wireplumber", "pipewire-pulse", "pipewire"])
        .status();
}

fn write_pipewire_and_restart(cfg: &AppConfig) -> Result<String, String> {
    let contributions: Vec<Vec<tideline_core::pipewire::directive::PipewireDirective>> =
        tideline_host::contribute::resolve_collisions(
            tideline_host::contribute::collect_pipewire_contributions(cfg),
        )
        .into_iter()
        .map(|c| c.directives)
        .collect();
    let backed_up = write_pipewire_conf_with_contributions(cfg, &contributions)?;
    restart_pipewire_stack();

    let mut msg = String::from("Applied. Audio engine restarted.");
    if !backed_up.is_empty() {
        msg.push_str(&format!(" Legacy files backed up: {}", backed_up.join(", ")));
    }
    Ok(msg)
}

fn channel_settings_equal(a: &ChannelCfg, b: &ChannelCfg) -> bool {
    a.name == b.name
        && a.kind == b.kind
        && a.hp_node == b.hp_node
        && a.sp_node == b.sp_node
        && a.programs == b.programs
        && a.sources == b.sources
        && a.physical_source == b.physical_source
}

fn try_soft_apply(old: &AppConfig, new: &AppConfig) -> Option<Vec<ChannelCfg>> {
    if old.mixes != new.mixes {
        return None;
    }
    let new_by_name: HashMap<&str, &ChannelCfg> =
        new.channels.iter().map(|c| (c.name.as_str(), c)).collect();
    for old_ch in &old.channels {
        let new_ch = new_by_name.get(old_ch.name.as_str())?;
        if !channel_settings_equal(old_ch, new_ch) {
            return None;
        }
    }
    let old_names: std::collections::HashSet<&str> =
        old.channels.iter().map(|c| c.name.as_str()).collect();
    Some(
        new.channels
            .iter()
            .filter(|c| !old_names.contains(c.name.as_str()))
            .cloned()
            .collect(),
    )
}

fn pw_cli_load(module: &str, args: &str) -> Result<(), String> {
    let out = Command::new("pw-cli")
        .args(["load-module", module, args])
        .output()
        .map_err(|e| format!("pw-cli load-module failed to spawn: {}", e))?;
    if !out.status.success() {
        return Err(format!(
            "pw-cli load-module {} failed: {}",
            module,
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    Ok(())
}

fn pactl_load_null_sink(sink_name: &str, description: &str) -> Result<(), String> {
    let args = format!(
        "sink_name={} sink_properties=device.description=\"{}\" channel_map=front-left,front-right",
        sink_name,
        description.replace('"', "'")
    );
    let out = Command::new("pactl")
        .args(["load-module", "module-null-sink", &args])
        .output()
        .map_err(|e| format!("pactl load-module failed to spawn: {}", e))?;
    if !out.status.success() {
        return Err(format!(
            "pactl load-module module-null-sink failed: {}",
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    Ok(())
}

fn load_channel_modules(cfg: &AppConfig, ch: &ChannelCfg) -> Result<(), String> {
    if ch.kind == ChannelKind::PhysicalInput {
        if ch.physical_source.is_empty() {
            return Ok(());
        }
        let existing: HashSet<String> = fetch_sink_inputs()
            .into_iter()
            .map(|s| s.node_name)
            .collect();
        for mix in &cfg.mixes {
            for (i, target) in mix.sinks.iter().enumerate() {
                let pb  = mix_playback_node(ch, mix, i);
                if existing.contains(&pb) {
                    continue;
                }
                let cap = mix_capture_node(ch, mix, i);
                let args = format!(
                    "{{ capture.props = {{ node.name = \"{cap}\" target.object = \"{src}\" audio.position = \"FL,FR\" stream.dont-remix = true }} playback.props = {{ node.name = \"{pb}\" target.object = \"{target}\" audio.position = \"FL,FR\" }} }}",
                    cap = cap,
                    src = ch.physical_source,
                    pb = pb,
                    target = target,
                );
                pw_cli_load("libpipewire-module-loopback", &args)?;
            }
        }
        return Ok(());
    }
    let sink_name = sink_node_for_channel(ch);
    pactl_load_null_sink(&sink_name, &ch.name)?;

    match ch.kind {
        ChannelKind::Output => {
            for mix in &cfg.mixes {
                for (i, target) in mix.sinks.iter().enumerate() {
                    let cap = mix_capture_node(ch, mix, i);
                    let pb  = mix_playback_node(ch, mix, i);
                    let args = format!(
                        "{{ capture.props = {{ node.name = \"{cap}\" target.object = \"{sink_name}\" audio.position = \"FL,FR\" stream.dont-remix = true stream.capture.sink = true }} playback.props = {{ node.name = \"{pb}\" target.object = \"{target}\" audio.position = \"FL,FR\" }} }}",
                        cap = cap,
                        sink_name = sink_name,
                        pb = pb,
                        target = target,
                    );
                    pw_cli_load("libpipewire-module-loopback", &args)?;
                }
            }
        }
        ChannelKind::Input => {
            let s = slug(&ch.name);
            for (i, src) in ch.sources.iter().enumerate() {
                let args = format!(
                    "{{ capture.props = {{ node.name = \"capture.{slug}-src-{i}\" target.object = \"{src}\" audio.position = \"FL,FR\" stream.dont-remix = true }} playback.props = {{ node.name = \"playback.{slug}-src-{i}\" target.object = \"{sink_name}\" audio.position = \"FL,FR\" }} }}",
                    slug = s,
                    i = i,
                    src = src,
                    sink_name = sink_name,
                );
                pw_cli_load("libpipewire-module-loopback", &args)?;
            }
        }
        ChannelKind::PhysicalInput => {}
    }
    Ok(())
}

fn reapply_mix_enabled_state(state: &State<'_, AppState>) {
    let cfg = state.config.lock().unwrap().clone();
    let stored = state.mix_enabled.lock().unwrap().clone();
    let map = mix_enabled_with_defaults(&stored, &cfg.mixes);
    write_mix_enabled(&map);
    apply_mix_enabled(&map, &cfg);
    *state.mix_enabled.lock().unwrap() = map;
}

#[tauri::command]
fn get_sink_inputs() -> Vec<SinkInput> {
    fetch_sink_inputs()
}

#[tauri::command]
fn set_volume(index: u32, pct: u32) {
    pactl(&["set-sink-input-volume", &index.to_string(), &format!("{}%", pct)]);
}

#[tauri::command]
fn set_mute(index: u32, muted: bool) {
    pactl(&["set-sink-input-mute", &index.to_string(), if muted { "1" } else { "0" }]);
}

#[tauri::command]
fn set_sink_volume(name: String, pct: u32) {
    pactl(&["set-sink-volume", &name, &format!("{}%", pct)]);
}

#[tauri::command]
fn set_sink_mute(name: String, muted: bool) {
    pactl(&["set-sink-mute", &name, if muted { "1" } else { "0" }]);
}

#[tauri::command]
fn get_sink_state(name: String) -> Option<(u32, bool)> {
    let raw = pactl_output(&["-f", "json", "list", "sinks"]);
    let json: serde_json::Value = serde_json::from_str(&raw).unwrap_or_default();
    for item in json.as_array().unwrap_or(&vec![]) {
        if item["name"].as_str() == Some(&name) {
            let muted = item["mute"].as_bool().unwrap_or(false);
            let volume = item["volume"]
                .as_object()
                .and_then(|m| m.values().next())
                .and_then(|v| v["value_percent"].as_str())
                .and_then(|s| s.trim_end_matches('%').parse::<u32>().ok())
                .unwrap_or(100);
            return Some((volume, muted));
        }
    }
    None
}

#[tauri::command]
fn list_sources() -> Vec<SourceInfo> {
    fetch_sources()
}

#[tauri::command]
fn set_source_volume(name: String, pct: u32) {
    pactl(&["set-source-volume", &name, &format!("{}%", pct)]);
}

#[tauri::command]
fn set_source_mute(name: String, muted: bool) {
    pactl(&["set-source-mute", &name, if muted { "1" } else { "0" }]);
}

#[tauri::command]
fn get_source_state(name: String) -> Option<(u32, bool)> {
    let raw = pactl_output(&["-f", "json", "list", "sources"]);
    let json: serde_json::Value = serde_json::from_str(&raw).unwrap_or_default();
    for item in json.as_array().unwrap_or(&vec![]) {
        if item["name"].as_str() == Some(&name) {
            let muted = item["mute"].as_bool().unwrap_or(false);
            let volume = item["volume"]
                .as_object()
                .and_then(|m| m.values().next())
                .and_then(|v| v["value_percent"].as_str())
                .and_then(|s| s.trim_end_matches('%').parse::<u32>().ok())
                .unwrap_or(100);
            return Some((volume, muted));
        }
    }
    None
}

#[tauri::command]
fn list_hardware_inputs() -> Vec<SourceInfo> {
    fetch_sources()
        .into_iter()
        .filter(|s| !s.name.ends_with(".monitor"))
        .collect()
}

#[tauri::command]
fn list_running_apps() -> Vec<RunningApp> {
    fetch_running_apps()
}

fn desktop_search_dirs() -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = Vec::new();
    if let Ok(home) = std::env::var("HOME") {
        dirs.push(PathBuf::from(format!("{home}/.local/share/applications")));
        dirs.push(PathBuf::from(format!("{home}/.local/share/flatpak/exports/share/applications")));
    }
    dirs.push(PathBuf::from("/usr/local/share/applications"));
    dirs.push(PathBuf::from("/usr/share/applications"));
    dirs.push(PathBuf::from("/var/lib/flatpak/exports/share/applications"));
    dirs.push(PathBuf::from("/var/lib/snapd/desktop/applications"));
    dirs
}

fn parse_desktop_entry(content: &str) -> HashMap<String, String> {
    let mut out: HashMap<String, String> = HashMap::new();
    let mut in_main = false;
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') {
            in_main = trimmed == "[Desktop Entry]";
            continue;
        }
        if !in_main || trimmed.is_empty() || trimmed.starts_with('#') { continue; }
        if let Some((k, v)) = trimmed.split_once('=') {
            out.insert(k.trim().to_string(), v.trim().to_string());
        }
    }
    out
}

fn binary_name_from_command(cmd: &str) -> Option<String> {
    let first = cmd.split_whitespace().find(|t| !t.starts_with('%') && !t.starts_with("env="))?;
    let path = std::path::Path::new(first);
    Some(path.file_name()?.to_string_lossy().to_string())
}

fn strip_common_suffixes(name: &str) -> String {
    let lower = name.to_ascii_lowercase();
    for suffix in ["-stable", "-bin", "-beta", "-nightly", "-dev", "-canary", "-unstable"] {
        if let Some(stripped) = lower.strip_suffix(suffix) {
            return stripped.to_string();
        }
    }
    lower
}

fn find_icon_name_for_binary(binary: &str) -> Option<String> {
    let needle = binary.to_ascii_lowercase();
    let needle_stripped = strip_common_suffixes(binary);

    let mut candidates: Vec<(u32, String)> = Vec::new();

    for dir in desktop_search_dirs() {
        let entries = match fs::read_dir(&dir) {
            Ok(e) => e,
            Err(_) => continue,
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) != Some("desktop") { continue; }
            let content = match fs::read_to_string(&path) {
                Ok(c) => c,
                Err(_) => continue,
            };
            let de = parse_desktop_entry(&content);
            if de.get("NoDisplay").map(|v| v == "true").unwrap_or(false) { continue; }
            let cmd = match de.get("Exec") {
                Some(e) => e,
                None => continue,
            };
            let bin = match binary_name_from_command(cmd) {
                Some(b) => b,
                None => continue,
            };
            let icon = match de.get("Icon") {
                Some(i) if !i.is_empty() => i.clone(),
                _ => continue,
            };

            let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
            let bin_lower = bin.to_ascii_lowercase();
            let bin_stripped = strip_common_suffixes(&bin);
            let stem_lower = stem.to_ascii_lowercase();
            let stem_last = stem_lower.rsplit('.').next().unwrap_or(&stem_lower).to_string();
            let stem_stripped = strip_common_suffixes(&stem_last);
            let wm_class = de.get("StartupWMClass").map(|s| s.to_ascii_lowercase()).unwrap_or_default();

            let priority: Option<u32> = if bin_lower == needle || stem_last == needle {
                Some(0)
            } else if wm_class == needle {
                Some(1)
            } else if bin_stripped == needle_stripped || stem_stripped == needle_stripped {
                Some(2)
            } else if needle.len() >= 3 && (
                stem_last.starts_with(&needle) || stem_last.ends_with(&needle)
                || bin_lower.starts_with(&needle) || bin_lower.ends_with(&needle)
            ) {
                Some(3)
            } else if needle.len() >= 4 && (
                stem_last.contains(&needle) || bin_lower.contains(&needle)
                || needle.contains(&stem_last) || needle.contains(&bin_lower)
            ) {
                Some(4)
            } else {
                None
            };

            if let Some(p) = priority {
                candidates.push((p, icon));
                if p == 0 { break; }
            }
        }
        if candidates.iter().any(|(p, _)| *p == 0) { break; }
    }

    candidates.sort_by_key(|(p, _)| *p);
    candidates.into_iter().next().map(|(_, icon)| icon)
}

fn resolve_icon_to_data_url(icon: &str) -> Option<String> {
    use base64::Engine;
    let path: PathBuf = if icon.starts_with('/') {
        PathBuf::from(icon)
    } else {
        let lookup = freedesktop_icons::lookup(icon)
            .with_size(64)
            .with_cache()
            .find();
        match lookup {
            Some(p) => p,
            None => freedesktop_icons::lookup(icon).with_cache().find()?,
        }
    };
    let bytes = fs::read(&path).ok()?;
    let mime = match path.extension().and_then(|s| s.to_str()) {
        Some("svg") => "image/svg+xml",
        Some("png") => "image/png",
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("xpm") => return None,
        _ => return None,
    };
    let encoded = base64::engine::general_purpose::STANDARD.encode(&bytes);
    Some(format!("data:{};base64,{}", mime, encoded))
}

#[tauri::command]
fn resolve_app_icons(binaries: Vec<String>) -> HashMap<String, Option<String>> {
    let mut out: HashMap<String, Option<String>> = HashMap::new();
    for bin in binaries {
        if bin.is_empty() { out.insert(bin, None); continue; }
        let resolved = find_icon_name_for_binary(&bin)
            .and_then(|n| resolve_icon_to_data_url(&n));
        out.insert(bin, resolved);
    }
    out
}

#[tauri::command]
fn watch_levels(sources: Vec<String>, monitor: State<'_, Arc<LevelMonitor>>) {
    monitor.watch(sources);
}

#[tauri::command]
fn get_card_for_source(source: String) -> Option<u32> {
    let raw = pactl_output(&["-f", "json", "list", "sources"]);
    let json: serde_json::Value = serde_json::from_str(&raw).unwrap_or_default();
    for item in json.as_array().unwrap_or(&vec![]) {
        if item["name"].as_str() == Some(&source) {
            return item["properties"]["alsa.card"]
                .as_str()
                .and_then(|s| s.parse::<u32>().ok());
        }
    }
    None
}

#[tauri::command]
fn get_card_for_sink(sink: String) -> Option<u32> {
    let raw = pactl_output(&["-f", "json", "list", "sinks"]);
    let json: serde_json::Value = serde_json::from_str(&raw).unwrap_or_default();
    for item in json.as_array().unwrap_or(&vec![]) {
        if item["name"].as_str() == Some(&sink) {
            return item["properties"]["alsa.card"]
                .as_str()
                .and_then(|s| s.parse::<u32>().ok());
        }
    }
    None
}

#[tauri::command]
fn list_card_controls(card: u32) -> Vec<CardControl> {
    let raw = amixer_output(&["-c", &card.to_string(), "-M", "scontents"]);
    parse_amixer_scontents(&raw)
}

#[tauri::command]
fn set_card_control_volume(card: u32, name: String, pct: u32) {
    let _ = Command::new("amixer")
        .args(["-c", &card.to_string(), "-M", "sset", &name, &format!("{}%", pct)])
        .status();
}

#[tauri::command]
fn set_card_control_mute(card: u32, name: String, muted: bool) {
    let _ = Command::new("amixer")
        .args([
            "-c",
            &card.to_string(),
            "sset",
            &name,
            if muted { "off" } else { "on" },
        ])
        .status();
}

#[tauri::command]
fn get_mix_enabled(state: State<'_, AppState>) -> HashMap<String, bool> {
    let cfg = state.config.lock().unwrap();
    let stored = state.mix_enabled.lock().unwrap();
    mix_enabled_with_defaults(&stored, &cfg.mixes)
}

#[tauri::command]
fn set_mix_enabled(id: String, enabled: bool, app: AppHandle, state: State<'_, AppState>) {
    let cfg = state.config.lock().unwrap().clone();
    let mut map = mix_enabled_with_defaults(&state.mix_enabled.lock().unwrap(), &cfg.mixes);
    map.insert(id, enabled);
    write_mix_enabled(&map);
    apply_mix_enabled(&map, &cfg);
    *state.mix_enabled.lock().unwrap() = map;
    refresh_tray_menu(&app);
}

fn parse_accelerator(accel: &str) -> Result<Shortcut, String> {
    accel.parse::<Shortcut>().map_err(|e| format!("invalid shortcut '{}': {}", accel, e))
}

fn channel_mute(app: &AppHandle, channel: &str) {
    let cfg = app.state::<AppState>().config.lock().unwrap().clone();
    let Some(ch) = cfg.channels.iter().find(|c| c.name == channel) else { return };
    if ch.kind == ChannelKind::PhysicalInput {
        if ch.physical_source.is_empty() { return; }
        let cur = get_source_state(ch.physical_source.clone()).map(|(_, m)| m).unwrap_or(false);
        pactl(&["set-source-mute", &ch.physical_source, if !cur { "1" } else { "0" }]);
    } else {
        let sink = sink_node_for_channel(ch);
        let cur = get_sink_state(sink.clone()).map(|(_, m)| m).unwrap_or(false);
        pactl(&["set-sink-mute", &sink, if !cur { "1" } else { "0" }]);
    }
}

fn output_mute(sink: &str) {
    let cur = get_sink_state(sink.to_string()).map(|(_, m)| m).unwrap_or(false);
    pactl(&["set-sink-mute", sink, if !cur { "1" } else { "0" }]);
}

fn execute_keybind(app: &AppHandle, action: &KeybindAction) {
    match action {
        KeybindAction::ToggleOutputMute { sink } => output_mute(sink),
        KeybindAction::ToggleChannelMute { channel } => channel_mute(app, channel),
        KeybindAction::ToggleMixEnabled { mix_id } => {
            let state = app.state::<AppState>();
            let cfg = state.config.lock().unwrap().clone();
            let mut map = mix_enabled_with_defaults(&state.mix_enabled.lock().unwrap(), &cfg.mixes);
            let next = !*map.get(mix_id).unwrap_or(&true);
            map.insert(mix_id.clone(), next);
            write_mix_enabled(&map);
            apply_mix_enabled(&map, &cfg);
            *state.mix_enabled.lock().unwrap() = map;
            refresh_tray_menu(app);
        }
        KeybindAction::Plugin { plugin_id, action_id } => {
            let registry = app.state::<Arc<tideline_host::PluginRegistry>>().inner().clone();
            let plugin_id = plugin_id.clone();
            let action_id = action_id.clone();
            tauri::async_runtime::spawn(async move {
                if let Err(e) = plugins::dispatch_plugin_keybind(&registry, &plugin_id, &action_id).await {
                    eprintln!("plugin keybind dispatch failed: {e}");
                }
            });
        }
    }
}

fn dispatch_shortcut(app: &AppHandle, shortcut: &Shortcut) {
    let cfg = app.state::<AppState>().config.lock().unwrap().clone();
    for (accel, action) in &cfg.keybinds {
        if let Ok(parsed) = parse_accelerator(accel) {
            if parsed == *shortcut {
                execute_keybind(app, action);
                return;
            }
        }
    }
}

fn register_all_keybinds(app: &AppHandle) {
    let cfg = app.state::<AppState>().config.lock().unwrap().clone();
    let gs = app.global_shortcut();
    let _ = gs.unregister_all();
    for accel in cfg.keybinds.keys() {
        match parse_accelerator(accel) {
            Ok(s) => {
                if let Err(e) = gs.register(s) {
                    eprintln!("failed to register shortcut '{}': {}", accel, e);
                }
            }
            Err(e) => eprintln!("{}", e),
        }
    }
}

#[tauri::command]
fn set_keybind(accelerator: String, action: KeybindAction, app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    parse_accelerator(&accelerator)?;
    {
        let mut cfg = state.config.lock().unwrap();
        cfg.keybinds.insert(accelerator.clone(), action);
        save_config_to_disk(&cfg)?;
    }
    register_all_keybinds(&app);
    Ok(())
}

#[tauri::command]
fn clear_keybind(accelerator: String, app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    {
        let mut cfg = state.config.lock().unwrap();
        cfg.keybinds.remove(&accelerator);
        save_config_to_disk(&cfg)?;
    }
    register_all_keybinds(&app);
    Ok(())
}

#[tauri::command]
fn get_config(state: State<'_, AppState>) -> AppConfig {
    state.config.lock().unwrap().clone()
}

#[tauri::command]
fn save_config(config: AppConfig, app: AppHandle, state: State<'_, AppState>) -> Result<String, String> {
    save_config_to_disk(&config)?;

    let old = state.config.lock().unwrap().clone();

    let msg = if let Some(added) = try_soft_apply(&old, &config) {
        let contributions: Vec<Vec<tideline_core::pipewire::directive::PipewireDirective>> =
            tideline_host::contribute::resolve_collisions(
                tideline_host::contribute::collect_pipewire_contributions(&config),
            )
            .into_iter()
            .map(|c| c.directives)
            .collect();
        let backed_up = write_pipewire_conf_with_contributions(&config, &contributions)?;
        let mut soft_failed: Option<String> = None;
        for ch in &added {
            if let Err(e) = load_channel_modules(&config, ch) {
                soft_failed = Some(e);
                break;
            }
        }
        if soft_failed.is_none() {
            let added_names: HashSet<&str> =
                added.iter().map(|c| c.name.as_str()).collect();
            for ch in &config.channels {
                if ch.kind != ChannelKind::PhysicalInput { continue; }
                if added_names.contains(ch.name.as_str()) { continue; }
                if let Err(e) = load_channel_modules(&config, ch) {
                    soft_failed = Some(e);
                    break;
                }
            }
        }
        if let Some(err) = soft_failed {
            eprintln!("soft apply failed ({}); falling back to restart", err);
            restart_pipewire_stack();
            let mut m = String::from("Applied. Audio engine restarted (soft apply failed).");
            if !backed_up.is_empty() {
                m.push_str(&format!(" Legacy files backed up: {}", backed_up.join(", ")));
            }
            m
        } else {
            std::thread::sleep(std::time::Duration::from_millis(300));
            let mut m = if added.is_empty() {
                String::from("Applied. No engine changes.")
            } else {
                format!("Applied. {} channel(s) added without restart.", added.len())
            };
            if !backed_up.is_empty() {
                m.push_str(&format!(" Legacy files backed up: {}", backed_up.join(", ")));
            }
            m
        }
    } else {
        write_pipewire_and_restart(&config)?
    };

    *state.config.lock().unwrap() = config.clone();
    reapply_mix_enabled_state(&state);
    refresh_tray_menu(&app);
    register_all_keybinds(&app);
    Ok(msg)
}

#[tauri::command]
fn save_config_quiet(config: AppConfig, app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    save_config_to_disk(&config)?;
    write_app_routing(&config)?;
    *state.config.lock().unwrap() = config;
    register_all_keybinds(&app);
    let _ = Command::new("systemctl")
        .args(["--user", "restart", "wireplumber"])
        .status();
    Ok(())
}

#[tauri::command]
fn set_channel_icon(name: String, icon: String, state: State<'_, AppState>) -> Result<(), String> {
    let mut cfg = state.config.lock().unwrap().clone();
    let mut found = false;
    for c in cfg.channels.iter_mut() {
        if c.name == name {
            c.icon = icon.clone();
            found = true;
            break;
        }
    }
    if !found {
        return Err(format!("channel '{}' not found", name));
    }
    save_config_to_disk(&cfg)?;
    *state.config.lock().unwrap() = cfg;
    Ok(())
}

#[tauri::command]
fn reorder_channels(order: Vec<String>, state: State<'_, AppState>) -> Result<(), String> {
    let mut cfg = state.config.lock().unwrap().clone();
    let mut by_name: HashMap<String, ChannelCfg> = cfg
        .channels
        .drain(..)
        .map(|c| (c.name.clone(), c))
        .collect();
    let mut next: Vec<ChannelCfg> = Vec::with_capacity(by_name.len());
    for name in &order {
        if let Some(c) = by_name.remove(name) {
            next.push(c);
        }
    }
    for (_, c) in by_name.into_iter() {
        next.push(c);
    }
    cfg.channels = next;
    save_config_to_disk(&cfg)?;
    *state.config.lock().unwrap() = cfg;
    Ok(())
}

#[tauri::command]
fn list_sinks() -> Vec<SinkInfo> {
    fetch_sinks()
}

#[tauri::command]
fn list_sink_input_nodes() -> Vec<String> {
    fetch_sink_inputs().into_iter().map(|s| s.node_name).collect()
}

#[tauri::command]
fn window_minimize(window: tauri::Window) -> Result<(), String> {
    window.minimize().map_err(|e| e.to_string())
}

#[tauri::command]
fn window_hide(window: tauri::Window) -> Result<(), String> {
    window.hide().map_err(|e| e.to_string())
}

#[tauri::command]
fn window_drag(window: tauri::Window) -> Result<(), String> {
    window.start_dragging().map_err(|e| e.to_string())
}

#[derive(Default)]
struct TrayItemsCache(Mutex<Vec<tideline_host::contributions::TrayItemContribution>>);

fn build_tray_menu(
    app: &AppHandle<Wry>,
    cfg: &AppConfig,
    enabled: &HashMap<String, bool>,
) -> tauri::Result<Menu<Wry>> {
    let menu = Menu::new(app)?;
    let mut mix_items: Vec<CheckMenuItem<Wry>> = Vec::with_capacity(cfg.mixes.len());
    for mix in &cfg.mixes {
        let on = *enabled.get(&mix.id).unwrap_or(&true);
        let item = CheckMenuItem::with_id(
            app,
            format!("mix:{}", mix.id),
            &mix.name,
            true,
            on,
            None::<&str>,
        )?;
        mix_items.push(item);
    }
    for item in &mix_items {
        menu.append(item)?;
    }
    if !cfg.mixes.is_empty() {
        let sep = PredefinedMenuItem::separator(app)?;
        menu.append(&sep)?;
    } else {
        let empty = MenuItem::with_id(app, "no_mixes", "No mixes configured", false, None::<&str>)?;
        menu.append(&empty)?;
        let sep = PredefinedMenuItem::separator(app)?;
        menu.append(&sep)?;
    }
    let mut tray_items = app
        .try_state::<TrayItemsCache>()
        .map(|c| c.inner().0.lock().unwrap().clone())
        .unwrap_or_default();
    tray_items.sort_by(|a, b| {
        b.priority
            .cmp(&a.priority)
            .then_with(|| a.plugin_id.cmp(&b.plugin_id))
    });
    if !tray_items.is_empty() {
        for item in &tray_items {
            let id = format!("plugin:{}:{}", item.plugin_id, item.item_id);
            let entry = MenuItem::with_id(app, id, &item.label, true, None::<&str>)?;
            menu.append(&entry)?;
        }
        let sep = PredefinedMenuItem::separator(app)?;
        menu.append(&sep)?;
    }
    let open_label = format!("Open {APP_TITLE}");
    let open = MenuItem::with_id(app, "open", &open_label, true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    menu.append(&open)?;
    menu.append(&quit)?;
    Ok(menu)
}

fn refresh_tray_menu(app: &AppHandle<Wry>) {
    let state = app.state::<AppState>();
    let cfg = state.config.lock().unwrap().clone();
    let enabled = mix_enabled_with_defaults(&state.mix_enabled.lock().unwrap(), &cfg.mixes);
    if let Ok(menu) = build_tray_menu(app, &cfg, &enabled) {
        if let Some(tray) = app.tray_by_id(TIDELINE_TRAY_ID) {
            let _ = tray.set_menu(Some(menu));
        }
    }
}

#[cfg(target_os = "linux")]
#[allow(dead_code)]
mod glog_filter {
    use std::ffi::{c_char, c_void, CStr};

    type GLogLevelFlags = u32;
    type GLogFunc = extern "C" fn(*const c_char, GLogLevelFlags, *const c_char, *mut c_void);

    extern "C" {
        fn g_log_set_default_handler(log_func: GLogFunc, user_data: *mut c_void) -> GLogFunc;
        fn g_log_default_handler(
            log_domain: *const c_char,
            log_level: GLogLevelFlags,
            message: *const c_char,
            user_data: *mut c_void,
        );
    }

    extern "C" fn handler(
        log_domain: *const c_char,
        log_level: GLogLevelFlags,
        message: *const c_char,
        user_data: *mut c_void,
    ) {
        if !log_domain.is_null() {
            let domain = unsafe { CStr::from_ptr(log_domain) };
            if domain.to_bytes() == b"libayatana-appindicator" {
                return;
            }
        }
        unsafe { g_log_default_handler(log_domain, log_level, message, user_data) };
    }

    pub fn install() {
        unsafe { g_log_set_default_handler(handler, std::ptr::null_mut()) };
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    #[cfg(target_os = "linux")]
    {
        // KWin 6.6 + NVIDIA + webkit2gtk 2.52 trip wp_linux_drm_syncobj_surface_v1
        // (Wayland Error 71). Routing webkit through its non-DMABUF compositor
        // path keeps the surface protocol happy.
        std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
        std::env::set_var("WEBKIT_FORCE_COMPOSITING_MODE", "1");
        glib_log::suppress_upstream_warnings();
    }

    let cfg = load_config();
    let stored_enabled = read_mix_enabled();
    let mix_enabled = mix_enabled_with_defaults(&stored_enabled, &cfg.mixes);
    let state = AppState {
        mix_enabled: Mutex::new(mix_enabled),
        config: Mutex::new(cfg),
    };

    tauri::Builder::default()
        .register_uri_scheme_protocol("tideline-plugin", |ctx, req| {
            plugins::handle_request(ctx, req)
        })
        .plugin(tauri_plugin_opener::init())
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, shortcut, event| {
                    if event.state() != ShortcutState::Pressed { return; }
                    dispatch_shortcut(app, shortcut);
                })
                .build(),
        )
        .manage(state)
        .setup(|app| {
            let backend = ensure_audio_backend();
            if !backend.started_services.is_empty() {
                eprintln!("started audio services: {}", backend.started_services.join(", "));
                if !backend.affected_apps.is_empty() {
                    eprintln!("apps that may need restart: {}", backend.affected_apps.join(", "));
                }
            }
            for err in &backend.errors {
                eprintln!("audio backend warning: {}", err);
            }
            match (&backend.server_name, backend.on_pipewire) {
                (Some(name), true) => eprintln!("audio backend ready: {}", name),
                (Some(name), false) => eprintln!("audio backend ready (non-pipewire): {}", name),
                (None, _) => eprintln!("audio backend not detected — pactl commands will fail"),
            }
            let _ = INITIAL_BACKEND.set(backend);

            app.manage(Arc::new(LevelMonitor::new(app.handle().clone())));
            routing::spawn(app.handle().clone());
            register_all_keybinds(app.handle());

            let plugin_registry = Arc::new(tideline_host::PluginRegistry::new());
            {
                let registry = plugin_registry.clone();
                tauri::async_runtime::spawn(async move {
                    registry
                        .set_backend(std::sync::Arc::new(plugins::backend::TauriHostBackend))
                        .await;
                });
            }
            app.manage(plugin_registry.clone());
            let iframe_bridge = tideline_host::IframeBridge::new(plugin_registry.clone());
            app.manage(iframe_bridge);
            let mut iframe_rx = plugin_registry.subscribe_iframe_messages();
            let app_handle_for_iframe = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                while let Ok(msg) = iframe_rx.recv().await {
                    let _ = plugins::forward_to_iframe(
                        &app_handle_for_iframe,
                        &msg.plugin_id,
                        &msg.surface_id,
                        msg.payload,
                    );
                }
            });
            plugins::spawn_contributions_relay(app.handle(), plugin_registry.clone());

            {
                let registry = plugin_registry.clone();
                tauri::async_runtime::spawn(async move {
                    if std::env::var_os("TIDELINE_DEV_PLUGINS_DIR").is_none() {
                        let mut found_dev = false;
                        if let Ok(cwd) = std::env::current_dir() {
                            let mut search = cwd.as_path();
                            for _ in 0..4 {
                                let candidate = search.join("plugins");
                                let manifest = search.join("Cargo.toml");
                                if candidate.is_dir() && manifest.is_file() {
                                    if let Ok(toml) = std::fs::read_to_string(&manifest) {
                                        if toml.contains("[workspace]") {
                                            std::env::set_var(
                                                "TIDELINE_DEV_PLUGINS_DIR",
                                                &candidate,
                                            );
                                            eprintln!(
                                                "plugins: dev override set TIDELINE_DEV_PLUGINS_DIR={}",
                                                candidate.display()
                                            );
                                            found_dev = true;
                                            break;
                                        }
                                    }
                                }
                                match search.parent() {
                                    Some(p) => search = p,
                                    None => break,
                                }
                            }
                        }
                        if !found_dev {
                            let install_root =
                                tideline_host::paths::data_home().join("plugins");
                            match crate::plugins::bundled::extract_to(&install_root) {
                                Ok(n) if n > 0 => eprintln!(
                                    "plugins: extracted {n} bundled plugin(s) to {}",
                                    install_root.display()
                                ),
                                Ok(_) => eprintln!(
                                    "plugins: bundled plugins already up-to-date at {}",
                                    install_root.display()
                                ),
                                Err(e) => eprintln!("plugins: extract failed: {e}"),
                            }
                        }
                    }
                    match registry.discover().await {
                        Ok(n) => eprintln!("plugins: discovered {n} plugin(s)"),
                        Err(e) => {
                            eprintln!("plugins: discover failed: {e}");
                            return;
                        }
                    }
                    let ids: Vec<String> = registry.installed_ids().await;
                    for id in ids {
                        match registry.start(&id).await {
                            Ok(_rt) => eprintln!("plugins: started {id}"),
                            Err(e) => eprintln!("plugins: start {id} failed: {e}"),
                        }
                    }
                });
            }

            app.manage(TrayItemsCache::default());
            let mut tray_rx = plugin_registry.subscribe_contributions();
            let registry_for_tray = plugin_registry.clone();
            let app_handle_for_tray = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                while tray_rx.recv().await.is_ok() {
                    let items = registry_for_tray.contributions().await.tray_items;
                    if let Some(cache) = app_handle_for_tray.try_state::<TrayItemsCache>() {
                        *cache.inner().0.lock().unwrap() = items;
                    }
                    refresh_tray_menu(&app_handle_for_tray);
                }
            });

            let cfg = app.state::<AppState>().config.lock().unwrap().clone();
            for ch in &cfg.channels {
                if ch.kind == ChannelKind::PhysicalInput {
                    if let Err(e) = load_channel_modules(&cfg, ch) {
                        eprintln!("startup loopback load failed for {}: {}", ch.name, e);
                    }
                }
            }

            let cfg_for_vols = cfg.clone();
            thread::spawn(move || {
                thread::sleep(Duration::from_millis(900));
                let all_vols = read_all_volumes();
                let mix_ids: Vec<String> = cfg_for_vols.mixes.iter().map(|m| m.id.clone()).collect();
                for ch in &cfg_for_vols.channels {
                    if let Some(vols) = all_vols.get(&ch.name) {
                        apply_channel_volumes(&slug(&ch.name), vols, &mix_ids);
                    }
                }
            });
            let enabled = mix_enabled_with_defaults(
                &app.state::<AppState>().mix_enabled.lock().unwrap(),
                &cfg.mixes,
            );
            let menu = build_tray_menu(app.handle(), &cfg, &enabled)?;

            if let Some(w) = app.get_webview_window("main") {
                let _ = w.set_title(APP_TITLE);
            }

            TrayIconBuilder::with_id(TIDELINE_TRAY_ID)
                .icon(app.default_window_icon().unwrap().clone())
                .tooltip(APP_TITLE)
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| {
                    let id = event.id.as_ref();
                    match id {
                        "open" => {
                            if let Some(w) = app.get_webview_window("main") {
                                let _ = w.show();
                                let _ = w.set_focus();
                            }
                        }
                        "quit" => app.exit(0),
                        s if s.starts_with("mix:") => {
                            let mix_id = s.trim_start_matches("mix:").to_string();
                            let state = app.state::<AppState>();
                            let cfg = state.config.lock().unwrap().clone();
                            let mut map = mix_enabled_with_defaults(
                                &state.mix_enabled.lock().unwrap(),
                                &cfg.mixes,
                            );
                            let next = !*map.get(&mix_id).unwrap_or(&true);
                            map.insert(mix_id, next);
                            write_mix_enabled(&map);
                            apply_mix_enabled(&map, &cfg);
                            *state.mix_enabled.lock().unwrap() = map;
                            refresh_tray_menu(app);
                        }
                        s if s.starts_with("plugin:") => {
                            let rest = &s["plugin:".len()..];
                            if let Some((plugin_id, action_id)) = rest.split_once(':') {
                                let registry = app
                                    .state::<Arc<tideline_host::PluginRegistry>>()
                                    .inner()
                                    .clone();
                                let plugin_id = plugin_id.to_string();
                                let action_id = action_id.to_string();
                                tauri::async_runtime::spawn(async move {
                                    let _ = plugins::dispatch_plugin_keybind(
                                        &registry,
                                        &plugin_id,
                                        &action_id,
                                    )
                                    .await;
                                });
                            }
                        }
                        _ => {}
                    }
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        let app = tray.app_handle();
                        if let Some(w) = app.get_webview_window("main") {
                            let _ = w.show();
                            let _ = w.set_focus();
                        }
                    }
                })
                .build(app)?;

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_sink_inputs,
            set_volume,
            set_mute,
            set_sink_volume,
            set_sink_mute,
            get_sink_state,
            list_sources,
            list_hardware_inputs,
            set_source_volume,
            set_source_mute,
            get_source_state,
            list_running_apps,
            resolve_app_icons,
            watch_levels,
            get_card_for_source,
            get_card_for_sink,
            list_card_controls,
            set_card_control_volume,
            set_card_control_mute,
            get_mix_enabled,
            set_mix_enabled,
            set_keybind,
            clear_keybind,
            get_config,
            save_config,
            save_config_quiet,
            reorder_channels,
            set_channel_icon,
            list_sinks,
            list_sink_input_nodes,
            window_minimize,
            window_hide,
            window_drag,
            ensure_audio_backend_cmd,
            get_initial_backend_status,
            get_all_channel_volumes,
            set_channel_master_volume,
            set_channel_mix_volume,
            plugins::tideline_plugin_iframe_send,
            plugins::tideline_plugin_contributions,
            plugins::tideline_plugin_emit_event,
            plugins::tideline_plugin_request_permission,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
