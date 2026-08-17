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
    AppHandle, Emitter, Manager, State, Wry,
};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

const TIDELINE_TRAY_ID: &str = "tideline-tray";

#[cfg(debug_assertions)]
const APP_TITLE: &str = "Tideline - Dev";
#[cfg(not(debug_assertions))]
const APP_TITLE: &str = "Tideline";

const LEGACY_MIX_ENABLED_FILE: &str = "/tmp/tideline-mix-enabled.json";
const LEGACY_VOLUMES_FILE: &str = "/tmp/tideline-volumes.json";

/// Persistent location for per-channel mix mute / master mute / volume
/// state. Was previously in `/tmp` which the OS clears on reboot — that
/// caused mute toggles to silently revert on every reboot. Lives next
/// to the main config so backups capture it together.
fn volumes_path() -> std::path::PathBuf {
    tideline_core::config_io::home_dir().join(".config/tideline/volumes.json")
}

/// Persistent location for the per-mix on/off toggle map.
fn mix_enabled_path() -> std::path::PathBuf {
    tideline_core::config_io::home_dir().join(".config/tideline/mix-enabled.json")
}

/// One-shot migration: if the new config-dir file is missing but the
/// legacy `/tmp` file exists, move the legacy file into place. Skips
/// on read errors so a corrupted legacy file doesn't poison the new
/// location.
fn migrate_legacy_state_file(new_path: &std::path::Path, legacy_path: &str) {
    if new_path.exists() {
        return;
    }
    let legacy = std::path::Path::new(legacy_path);
    if !legacy.exists() {
        return;
    }
    if let Some(parent) = new_path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    if let Err(e) = fs::rename(legacy, new_path) {
        eprintln!(
            "[migrate] failed to move {} to {}: {e}",
            legacy_path,
            new_path.display()
        );
    } else {
        eprintln!("[migrate] moved {} to {}", legacy_path, new_path.display());
    }
}

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

/// Coalesces pipewire conf-rewrite requests from multiple call sites
/// (rack_changed bus events, direct attach_channel_data nudges) into a
/// single debounced rebuild. Held as Tauri state so any handler can poke
/// it without round-tripping through the event bus.
#[derive(Clone)]
pub struct PipewireRebuildTrigger {
    pub notify: std::sync::Arc<tokio::sync::Notify>,
}

impl PipewireRebuildTrigger {
    pub fn new() -> Self {
        Self {
            notify: std::sync::Arc::new(tokio::sync::Notify::new()),
        }
    }
    pub fn poke(&self) {
        self.notify.notify_one();
    }
}

impl Default for PipewireRebuildTrigger {
    fn default() -> Self {
        Self::new()
    }
}

use tideline_core::config_io::{load_config, save_config_to_disk, slug};

fn pactl_check(args: &[&str]) -> bool {
    Command::new("pactl")
        .args(args)
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

const EVT_SOURCE_MUTE: &str = "tideline:source_mute_changed";
const EVT_SINK_MUTE: &str = "tideline:sink_mute_changed";
const EVT_SOURCE_VOLUME: &str = "tideline:source_volume_changed";
const EVT_SINK_VOLUME: &str = "tideline:sink_volume_changed";
const EVT_CARD_CONTROL_VOLUME: &str = "tideline:card_control_volume_changed";
const EVT_SINK_INPUT_MUTE: &str = "tideline:sink_input_mute_changed";
const EVT_SINK_INPUT_VOLUME: &str = "tideline:sink_input_volume_changed";

fn emit_source_mute(app: &AppHandle, name: &str, muted: bool) {
    let _ = app.emit(
        EVT_SOURCE_MUTE,
        serde_json::json!({ "source_name": name, "muted": muted }),
    );
    refresh_tray_menu(app);
}
fn emit_sink_mute(app: &AppHandle, name: &str, muted: bool) {
    let _ = app.emit(
        EVT_SINK_MUTE,
        serde_json::json!({ "sink_name": name, "muted": muted }),
    );
    refresh_tray_menu(app);
}
fn emit_source_volume(app: &AppHandle, name: &str, volume_pct: u32) {
    let _ = app.emit(
        EVT_SOURCE_VOLUME,
        serde_json::json!({ "source_name": name, "volume_pct": volume_pct }),
    );
}
fn emit_sink_volume(app: &AppHandle, name: &str, volume_pct: u32) {
    let _ = app.emit(
        EVT_SINK_VOLUME,
        serde_json::json!({ "sink_name": name, "volume_pct": volume_pct }),
    );
}
fn emit_card_control_volume(app: &AppHandle, card: u32, name: &str, volume_pct: u32) {
    let _ = app.emit(
        EVT_CARD_CONTROL_VOLUME,
        serde_json::json!({ "card": card, "name": name, "volume_pct": volume_pct }),
    );
}
fn emit_sink_input_mute(app: &AppHandle, index: u32, muted: bool) {
    let _ = app.emit(
        EVT_SINK_INPUT_MUTE,
        serde_json::json!({ "index": index, "muted": muted }),
    );
}
fn emit_sink_input_volume(app: &AppHandle, index: u32, volume_pct: u32) {
    let _ = app.emit(
        EVT_SINK_INPUT_VOLUME,
        serde_json::json!({ "index": index, "volume_pct": volume_pct }),
    );
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

    let units = [
        "pipewire.socket",
        "pipewire-pulse.socket",
        "wireplumber.service",
    ];
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
        status
            .errors
            .push("pactl info still returns no server after start attempt".to_string());
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
            Some(SinkInput {
                index,
                node_name,
                muted,
                volume,
            })
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
            let description = item["description"].as_str().unwrap_or(&name).to_string();
            let muted = item["mute"].as_bool().unwrap_or(false);
            let volume_percent = item["volume"]
                .as_object()
                .and_then(|m| m.values().next())
                .and_then(|v| v["value_percent"].as_str())
                .and_then(|s| s.trim_end_matches('%').parse::<u32>().ok())
                .unwrap_or(100);
            Some(SinkInfo {
                name,
                description,
                muted,
                volume_percent,
            })
        })
        .collect()
}

/// Sink names present in the running graph, for filtering mix targets that
/// point at a device that is gone. `None` when pactl gave us nothing back.
fn available_sink_names() -> Option<std::collections::HashSet<String>> {
    let names: std::collections::HashSet<String> =
        fetch_sinks().into_iter().map(|s| s.name).collect();
    if names.is_empty() {
        // pipewire is down or still coming up; filtering here would strip
        // every loopback and write an empty conf.
        return None;
    }
    Some(names)
}

fn fetch_sources() -> Vec<SourceInfo> {
    let raw = pactl_output(&["-f", "json", "list", "sources"]);
    let json: serde_json::Value = serde_json::from_str(&raw).unwrap_or_default();

    json.as_array()
        .unwrap_or(&vec![])
        .iter()
        .filter_map(|item| {
            let name = item["name"].as_str()?.to_string();
            let description = item["description"].as_str().unwrap_or(&name).to_string();
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
        let binary = props["application.process.binary"]
            .as_str()
            .unwrap_or("")
            .to_string();
        if binary.is_empty() {
            continue;
        }
        if !seen.insert(binary.clone()) {
            continue;
        }
        let application_name = props["application.name"]
            .as_str()
            .unwrap_or(&binary)
            .to_string();
        let sink = item["sink"]
            .as_u64()
            .map(|n| n.to_string())
            .unwrap_or_default();
        out.push(RunningApp {
            binary,
            application_name,
            sink,
        });
    }
    out
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelVolumes {
    #[serde(default = "default_master_pct")]
    pub master: u32,
    #[serde(default)]
    pub mixes: HashMap<String, u32>,
    /// Master mute for the channel — applies across every mix the channel
    /// routes to. Pre-existing volumes files predate this field; default false.
    #[serde(default)]
    pub master_muted: bool,
    /// Per-mix mute. Distinct from `master_muted` so users can mute the mic
    /// from "Main Mix" while still sending it to "Voice Chat", etc. Defaults
    /// to empty (i.e. unmuted) for older volumes files.
    #[serde(default)]
    pub mix_muted: HashMap<String, bool>,
}

fn default_master_pct() -> u32 {
    100
}

impl Default for ChannelVolumes {
    fn default() -> Self {
        Self {
            master: 100,
            mixes: HashMap::new(),
            master_muted: false,
            mix_muted: HashMap::new(),
        }
    }
}

fn read_all_volumes() -> HashMap<String, ChannelVolumes> {
    let path = volumes_path();
    migrate_legacy_state_file(&path, LEGACY_VOLUMES_FILE);
    fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

/// Build the per-(channel, mix) mute table that the pipewire contributor
/// needs. effective mute = master_muted || per-mix muted, surfaced as
/// `MixMuteEntry { muted = true }` so the contributor skips the matching
/// post-loopback. unmuted pairs are emitted with `muted = false` for
/// completeness; the contributor only acts on muted=true entries.
pub fn build_mix_mutes(
    cfg: &tideline_core::model::AppConfig,
) -> Vec<tideline_sdk::contribute::MixMuteEntry> {
    let vols = read_all_volumes();
    let mut out = Vec::new();
    for ch in &cfg.channels {
        let entry = vols.get(&ch.name);
        for mix in &cfg.mixes {
            let master = entry.map(|e| e.master_muted).unwrap_or(false);
            let per_mix = entry
                .and_then(|e| e.mix_muted.get(&mix.id).copied())
                .unwrap_or(false);
            out.push(tideline_sdk::contribute::MixMuteEntry {
                channel_name: ch.name.clone(),
                mix_id: mix.id.clone(),
                muted: master || per_mix,
            });
        }
    }
    out
}

fn write_all_volumes(map: &HashMap<String, ChannelVolumes>) {
    let path = volumes_path();
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    if let Ok(s) = serde_json::to_string_pretty(map) {
        if let Err(e) = fs::write(&path, s) {
            eprintln!("[volumes] failed to write {}: {e}", path.display());
        }
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

fn apply_channel_volumes(
    app: &AppHandle,
    channel_slug: &str,
    vols: &ChannelVolumes,
    mix_ids: &[String],
) {
    // Mute is enforced here by clamping the post-loopback's playback
    // sink-input volume to 0%. pulse `set-sink-input-mute` did not
    // propagate reliably to pw-native loopback streams, but volume does.
    // master_muted OR per-mix muted -> 0%, otherwise master*mix.
    let snapshot = fetch_sink_inputs();
    for mix_id in mix_ids {
        let mix_pct = vols.mixes.get(mix_id).copied().unwrap_or(100);
        let muted = vols.master_muted || vols.mix_muted.get(mix_id).copied().unwrap_or(false);
        let final_pct = if muted {
            0
        } else {
            product_pct(vols.master, mix_pct)
        };
        let prefix = format!("playback.{}-{}-", channel_slug, mix_id);
        let indexes: Vec<u32> = snapshot
            .iter()
            .filter(|s| s.node_name.starts_with(&prefix))
            .map(|s| s.index)
            .collect();
        for idx in &indexes {
            if pactl_check(&[
                "set-sink-input-volume",
                &idx.to_string(),
                &format!("{}%", final_pct),
            ]) {
                emit_sink_input_volume(app, *idx, final_pct);
            }
        }
    }
}

/// Reapply persisted master/per-mix mute and volume to every sink-input.
/// Called after pipewire/wireplumber restart, where every sink-input is
/// freshly minted at default unmuted/100% — without this, "muted in the
/// mix" silently flips back to live audio whenever the rack changes.
pub fn reapply_all_channel_volumes_and_mutes(app: &AppHandle) {
    let Some(state) = app.try_state::<AppState>() else {
        eprintln!("[mute-restore] AppState missing, skipping");
        return;
    };
    let cfg = state.config.lock().unwrap().clone();
    let all_vols = read_all_volumes();
    let mix_ids: Vec<String> = cfg.mixes.iter().map(|m| m.id.clone()).collect();
    let mut applied = 0usize;
    for ch in &cfg.channels {
        if let Some(vols) = all_vols.get(&ch.name) {
            apply_channel_volumes(app, &slug(&ch.name), vols, &mix_ids);
            applied += 1;
        }
    }
    eprintln!(
        "[mute-restore] reapplied volumes/mutes for {} channels across {} mixes",
        applied,
        mix_ids.len()
    );
}

#[tauri::command]
fn get_all_channel_volumes() -> HashMap<String, ChannelVolumes> {
    read_all_volumes()
}

#[tauri::command]
fn set_channel_master_volume(
    app: AppHandle,
    channel: String,
    pct: u32,
    state: State<'_, AppState>,
) {
    let cfg = state.config.lock().unwrap().clone();
    let mix_ids: Vec<String> = cfg.mixes.iter().map(|m| m.id.clone()).collect();
    let mut all = read_all_volumes();
    let entry = all.entry(channel.clone()).or_default();
    entry.master = pct.min(100);
    let snap = entry.clone();
    write_all_volumes(&all);
    apply_channel_volumes(&app, &slug(&channel), &snap, &mix_ids);
}

#[tauri::command]
fn set_channel_mix_volume(app: AppHandle, channel: String, mix_id: String, pct: u32) {
    let mut all = read_all_volumes();
    let entry = all.entry(channel.clone()).or_default();
    entry.mixes.insert(mix_id.clone(), pct.min(100));
    let snap = entry.clone();
    write_all_volumes(&all);
    let final_pct = product_pct(snap.master, pct.min(100));
    for idx in sink_input_indexes_for_channel_mix(&slug(&channel), &mix_id) {
        if pactl_check(&[
            "set-sink-input-volume",
            &idx.to_string(),
            &format!("{}%", final_pct),
        ]) {
            emit_sink_input_volume(&app, idx, final_pct);
        }
    }
}

#[tauri::command]
fn set_channel_master_mute(
    app: AppHandle,
    channel: String,
    muted: bool,
    state: State<'_, AppState>,
) {
    let cfg = state.config.lock().unwrap().clone();
    let mix_ids: Vec<String> = cfg.mixes.iter().map(|m| m.id.clone()).collect();
    let mut all = read_all_volumes();
    let entry = all.entry(channel.clone()).or_default();
    entry.master_muted = muted;
    let snap = entry.clone();
    write_all_volumes(&all);
    // Apply mute by clamping volume to 0% on every post-loopback's
    // playback sink-input — see apply_channel_volumes. No conf rewrite,
    // no module destroy/load, no pipewire restart.
    apply_channel_volumes(&app, &slug(&channel), &snap, &mix_ids);
    for mix_id in &mix_ids {
        for idx in sink_input_indexes_for_channel_mix(&slug(&channel), mix_id) {
            emit_sink_input_mute(&app, idx, muted);
        }
    }
}

#[tauri::command]
fn set_channel_mix_mute(
    app: AppHandle,
    channel: String,
    mix_id: String,
    muted: bool,
    state: State<'_, AppState>,
) {
    let cfg = state.config.lock().unwrap().clone();
    let mix_ids: Vec<String> = cfg.mixes.iter().map(|m| m.id.clone()).collect();
    let mut all = read_all_volumes();
    let entry = all.entry(channel.clone()).or_default();
    entry.mix_muted.insert(mix_id.clone(), muted);
    let master_muted = entry.master_muted;
    let snap = entry.clone();
    let effective = master_muted || muted;
    write_all_volumes(&all);
    // Apply mute by clamping volume to 0% on this mix's post-loopback
    // sink-inputs — see apply_channel_volumes. No conf rewrite, no
    // module destroy/load, no pipewire restart. We pass all mix_ids so
    // master+mix combinations recompute correctly across every mix on
    // this channel.
    apply_channel_volumes(&app, &slug(&channel), &snap, &mix_ids);
    for idx in sink_input_indexes_for_channel_mix(&slug(&channel), &mix_id) {
        emit_sink_input_mute(&app, idx, effective);
    }
}

fn read_mix_enabled() -> HashMap<String, bool> {
    let path = mix_enabled_path();
    migrate_legacy_state_file(&path, LEGACY_MIX_ENABLED_FILE);
    fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str::<HashMap<String, bool>>(&s).ok())
        .unwrap_or_default()
}

fn write_mix_enabled(map: &HashMap<String, bool>) {
    let path = mix_enabled_path();
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    if let Ok(s) = serde_json::to_string(map) {
        if let Err(e) = fs::write(&path, s) {
            eprintln!("[mix-enabled] failed to write {}: {e}", path.display());
        }
    }
}

fn mix_enabled_with_defaults(
    stored: &HashMap<String, bool>,
    mixes: &[Mix],
) -> HashMap<String, bool> {
    mixes
        .iter()
        .map(|m| (m.id.clone(), *stored.get(&m.id).unwrap_or(&true)))
        .collect()
}

fn apply_mix_enabled(app: &AppHandle, enabled: &HashMap<String, bool>, cfg: &AppConfig) {
    let inputs = fetch_sink_inputs();
    let lookup: HashMap<&str, u32> = inputs
        .iter()
        .map(|i| (i.node_name.as_str(), i.index))
        .collect();

    for ch in &cfg.channels {
        if ch.kind != ChannelKind::Output {
            continue;
        }
        for mix in &cfg.mixes {
            let muted = !*enabled.get(&mix.id).unwrap_or(&true);
            let mute_arg = if muted { "1" } else { "0" };
            for (i, _) in mix.sinks.iter().enumerate() {
                let pb = mix_playback_node(ch, mix, i);
                if let Some(&idx) = lookup.get(pb.as_str()) {
                    if pactl_check(&["set-sink-input-mute", &idx.to_string(), mute_arg]) {
                        emit_sink_input_mute(app, idx, muted);
                    }
                }
            }
        }
    }
}

use tideline_core::pipewire::{
    mix_capture_node, mix_playback_node, sink_node_for_channel, write_app_routing,
    write_pipewire_conf_with_contributions,
};

async fn restart_pipewire_stack(registry: &Arc<tideline_host::PluginRegistry>) {
    // Snapshot current source mute state BEFORE restart so we can restore it
    // afterward. systemctl restart wireplumber wipes mute on every source —
    // a plugin-side restore via host:pipewire_restarted is racy because the
    // event can arrive before the plugin has resubscribed. The host doing
    // the snapshot itself is the only reliable path.
    let mute_snapshot = snapshot_source_mutes();
    registry
        .publish_host_event("host:pipewire_restarting", serde_json::json!({}))
        .await;
    let _ = Command::new("systemctl")
        .args([
            "--user",
            "restart",
            "wireplumber",
            "pipewire-pulse",
            "pipewire",
        ])
        .status();
    // Give the stack a moment to come back up before notifying plugins to
    // re-attach. Without this the JACK socket may not be ready when our
    // tries to reconnect.
    tokio::time::sleep(std::time::Duration::from_millis(800)).await;
    restore_source_mutes(&mute_snapshot);
    registry
        .publish_host_event("host:pipewire_restarted", serde_json::json!({}))
        .await;
}

/// Capture `(source_name, muted)` pairs via `pactl list sources`. Used as a
/// belt-and-suspenders mute restoration around pipewire restarts.
fn snapshot_source_mutes() -> Vec<(String, bool)> {
    let raw = pactl_output(&["-f", "json", "list", "sources"]);
    let json: serde_json::Value = serde_json::from_str(&raw).unwrap_or_default();
    let arr = match json.as_array() {
        Some(a) => a,
        None => return Vec::new(),
    };
    let mut out = Vec::new();
    for item in arr {
        let name = item.get("name").and_then(|v| v.as_str()).unwrap_or("");
        if name.is_empty() {
            continue;
        }
        let muted = item.get("mute").and_then(|v| v.as_bool()).unwrap_or(false);
        out.push((name.to_string(), muted));
    }
    eprintln!(
        "[pipewire] snapshot_source_mutes captured {} sources",
        out.len()
    );
    out
}

fn restore_source_mutes(snapshot: &[(String, bool)]) {
    let mut restored = 0usize;
    for (name, muted) in snapshot {
        if pactl_check(&["set-source-mute", name, if *muted { "1" } else { "0" }]) {
            restored += 1;
        }
    }
    eprintln!(
        "[pipewire] restore_source_mutes: restored {}/{}",
        restored,
        snapshot.len()
    );
}

/// Serializes pipewire conf rewrites + restarts. Concurrent callers
/// (rebuild worker, startup write, frontend command) would otherwise
/// stomp on each other: snapshot_source_mutes captures stale state,
/// pipewire restarts twice in rapid succession, and the second restart
/// kills JACK clients before the engine has finished re-instantiating
/// them from the first restart.
static PW_RESTART_MUTEX: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

pub async fn write_pipewire_and_restart(
    cfg: &AppConfig,
    registry: &Arc<tideline_host::PluginRegistry>,
) -> Result<String, String> {
    let _guard = PW_RESTART_MUTEX.lock().await;
    let channels_with_fx: Vec<&str> = cfg
        .channels
        .iter()
        .filter(|c| {
            c.plugin_data
                .get("tideline-effects")
                .and_then(|v| v.get("effects"))
                .and_then(|e| e.as_array())
                .map(|a| !a.is_empty())
                .unwrap_or(false)
        })
        .map(|c| c.name.as_str())
        .collect();
    eprintln!(
        "[pipewire] write_pipewire_and_restart: channels_total={} channels_with_fx={:?}",
        cfg.channels.len(),
        channels_with_fx
    );
    let mix_mutes = build_mix_mutes(cfg);
    let raw =
        tideline_host::contribute::collect_pipewire_contributions(registry, cfg, &mix_mutes).await;
    let raw_counts: Vec<(String, usize)> = raw
        .iter()
        .map(|c| (c.plugin_id.clone(), c.directives.len()))
        .collect();
    eprintln!("[pipewire] raw contributions: {:?}", raw_counts);
    let contributions: Vec<Vec<tideline_core::pipewire::directive::PipewireDirective>> =
        tideline_host::contribute::resolve_collisions(raw)
            .into_iter()
            .map(|c| c.directives)
            .collect();
    let post_counts: Vec<usize> = contributions.iter().map(|d| d.len()).collect();
    eprintln!(
        "[pipewire] post-resolve directive counts: {:?}",
        post_counts
    );
    // Skip restart entirely if the rendered conf is byte-identical to what's
    // already on disk. This collapses the rapid-fire rebuilds (multiple
    // attach_channel_data + rack_changed pokes within the same debounce
    // window) into a single state transition.
    let available = available_sink_names();
    let new_body = tideline_core::pipewire::build_pipewire_conf(
        cfg,
        &contributions,
        &mix_mutes,
        available.as_ref(),
    );
    let conf_path = tideline_core::config_io::pipewire_conf_dir()
        .join(tideline_core::pipewire::TIDELINE_PIPEWIRE_FILE);
    let unchanged = std::fs::read_to_string(&conf_path)
        .map(|existing| existing == new_body)
        .unwrap_or(false);
    if unchanged {
        eprintln!("[pipewire] conf unchanged — skipping restart, re-wiring fx links only");
        wire_fx_links(cfg).await;
        return Ok("Already current. (no restart)".into());
    }
    let backed_up = write_pipewire_conf_with_contributions(
        cfg,
        &contributions,
        &mix_mutes,
        available.as_ref(),
    )?;
    restart_pipewire_stack(registry).await;
    // Pipewire's autoconnect can't link to JACK clients (media.class=null),
    // so the contributor sets node.autoconnect=false on fx_node-targeted
    // loopback sides and we explicitly link them here. Retries because the
    // engine takes ~800ms-2s to re-instantiate JACK clients after restart.
    wire_fx_links(cfg).await;

    let mut msg = String::from("Applied. Audio engine restarted.");
    if !backed_up.is_empty() {
        msg.push_str(&format!(
            " Legacy files backed up: {}",
            backed_up.join(", ")
        ));
    }
    Ok(msg)
}

/// Creates explicit pipewire links between fx loopbacks and per-channel JACK
/// clients (`tideline-fx-{simple_uuid}`). Idempotent — pw-link returns
/// "File exists" on duplicate links, which we silently swallow.
///
/// Port naming: pipewire's `module-loopback` always names its ports
/// numerically (`input_0`, `input_1`, `output_0`, `output_1`) regardless
/// of `audio.position`. JACK clients expose channel-named ports
/// (`in_FL`, `in_FR`, `out_FL`, `out_FR`).
pub async fn wire_fx_links(cfg: &AppConfig) {
    let available = available_sink_names();
    let available = available.as_ref();
    let mut pairs: Vec<(String, String)> = Vec::new();
    for ch in &cfg.channels {
        let effects_data = ch.plugin_data.get("tideline-effects");
        // Wire whenever the channel has ANY effects in its rack, bypassed
        // or not. Per-effect and chain-level bypass are handled at runtime
        // by the JACK ProcessHandler — pipewire routing stays stable so
        // bypass toggles never reshape the conf or trigger restarts.
        let has_any_effects = effects_data
            .and_then(|v| v.get("effects"))
            .and_then(|e| e.as_array())
            .map(|a| !a.is_empty())
            .unwrap_or(false);
        if !has_any_effects {
            continue;
        }
        if matches!(ch.kind, ChannelKind::PhysicalInput) && ch.physical_source.is_empty() {
            continue;
        }

        let s = slug(&ch.name);
        let fx = format!("tideline-fx-{}", ch.uuid.simple());

        match ch.kind {
            ChannelKind::Output | ChannelKind::PhysicalInput => {
                pairs.push((
                    format!("playback.{s}-fx-pre:output_0"),
                    format!("{fx}:in_FL"),
                ));
                pairs.push((
                    format!("playback.{s}-fx-pre:output_1"),
                    format!("{fx}:in_FR"),
                ));
            }
            ChannelKind::Input => {
                for i in 0..ch.sources.len() {
                    pairs.push((
                        format!("playback.{s}-fx-src-{i}:output_0"),
                        format!("{fx}:in_FL"),
                    ));
                    pairs.push((
                        format!("playback.{s}-fx-src-{i}:output_1"),
                        format!("{fx}:in_FR"),
                    ));
                }
            }
        }

        match ch.kind {
            ChannelKind::Output | ChannelKind::PhysicalInput => {
                if matches!(ch.kind, ChannelKind::PhysicalInput) {
                    pairs.push((
                        format!("{fx}:out_FL"),
                        format!("capture.{s}-fx-virtual:input_0"),
                    ));
                    pairs.push((
                        format!("{fx}:out_FR"),
                        format!("capture.{s}-fx-virtual:input_1"),
                    ));
                    // playback.fx-virtual lands on fx_source (Audio/Source/Virtual).
                    // pipewire's loopback target.object only auto-routes to sinks,
                    // so the playback gets autoconnect=false in the contributor and
                    // wires explicitly here. without this, the playback falls back
                    // to the default sink (sink.system) and apps recording from
                    // fx_source see silence while audio leaks into the headphones path.
                    pairs.push((
                        format!("playback.{s}-fx-virtual:output_0"),
                        format!("fx_source.{s}:input_0"),
                    ));
                    pairs.push((
                        format!("playback.{s}-fx-virtual:output_1"),
                        format!("fx_source.{s}:input_1"),
                    ));
                }
                for mix in &cfg.mixes {
                    for (i, target) in mix.sinks.iter().enumerate() {
                        // the conf builder skips loopbacks to absent sinks, so
                        // their capture nodes dont exist to link to either.
                        if available.is_some_and(|set| !set.contains(target)) {
                            continue;
                        }
                        let cap = mix_capture_node(ch, mix, i);
                        pairs.push((format!("{fx}:out_FL"), format!("{cap}:input_0")));
                        pairs.push((format!("{fx}:out_FR"), format!("{cap}:input_1")));
                    }
                }
            }
            ChannelKind::Input => {
                pairs.push((
                    format!("{fx}:out_FL"),
                    format!("capture.{s}-fx-post:input_0"),
                ));
                pairs.push((
                    format!("{fx}:out_FR"),
                    format!("capture.{s}-fx-post:input_1"),
                ));
            }
        }
    }

    if pairs.is_empty() {
        return;
    }
    eprintln!("[fx-link] wiring {} pw-link pairs", pairs.len());

    let mut remaining = pairs;
    let total = remaining.len();
    for attempt in 0..30 {
        if remaining.is_empty() {
            break;
        }
        if attempt > 0 {
            tokio::time::sleep(std::time::Duration::from_millis(400)).await;
        }
        let mut next = Vec::new();
        for (out, inp) in remaining.drain(..) {
            match Command::new("pw-link").arg(&out).arg(&inp).output() {
                Ok(o) if o.status.success() => {}
                Ok(o) => {
                    let stderr = String::from_utf8_lossy(&o.stderr);
                    if stderr.contains("File exists") {
                    } else {
                        next.push((out, inp));
                    }
                }
                Err(_) => next.push((out, inp)),
            }
        }
        if next.len() != remaining.len() && !next.is_empty() {
            eprintln!(
                "[fx-link] attempt {attempt}: {} pending of {total}",
                next.len()
            );
        }
        remaining = next;
    }
    if !remaining.is_empty() {
        for (out, inp) in &remaining {
            eprintln!("[fx-link] gave up: {out} -> {inp}");
        }
    } else {
        eprintln!("[fx-link] all {total} pairs linked");
    }
}

/// Like `write_pipewire_and_restart` but DOES NOT restart pipewire. Used
/// when a change has already been applied incrementally via pw-cli
/// (e.g. per-mix mute toggle) and we just need the conf on disk to
/// reflect the new desired state for the next pipewire restart.
pub async fn write_pipewire_conf_only(
    cfg: &AppConfig,
    registry: &Arc<tideline_host::PluginRegistry>,
) -> Result<(), String> {
    let mix_mutes = build_mix_mutes(cfg);
    let raw =
        tideline_host::contribute::collect_pipewire_contributions(registry, cfg, &mix_mutes).await;
    let contributions: Vec<Vec<tideline_core::pipewire::directive::PipewireDirective>> =
        tideline_host::contribute::resolve_collisions(raw)
            .into_iter()
            .map(|c| c.directives)
            .collect();
    write_pipewire_conf_with_contributions(
        cfg,
        &contributions,
        &mix_mutes,
        available_sink_names().as_ref(),
    )?;
    Ok(())
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
                let pb = mix_playback_node(ch, mix, i);
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
                    let pb = mix_playback_node(ch, mix, i);
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

fn reapply_mix_enabled_state(app: &AppHandle, state: &State<'_, AppState>) {
    let cfg = state.config.lock().unwrap().clone();
    let stored = state.mix_enabled.lock().unwrap().clone();
    let map = mix_enabled_with_defaults(&stored, &cfg.mixes);
    write_mix_enabled(&map);
    apply_mix_enabled(app, &map, &cfg);
    *state.mix_enabled.lock().unwrap() = map;
}

#[tauri::command]
fn get_sink_inputs() -> Vec<SinkInput> {
    fetch_sink_inputs()
}

#[tauri::command]
fn set_volume(app: AppHandle, index: u32, pct: u32) {
    if pactl_check(&[
        "set-sink-input-volume",
        &index.to_string(),
        &format!("{}%", pct),
    ]) {
        emit_sink_input_volume(&app, index, pct);
    }
}

#[tauri::command]
fn set_mute(app: AppHandle, index: u32, muted: bool) {
    if pactl_check(&[
        "set-sink-input-mute",
        &index.to_string(),
        if muted { "1" } else { "0" },
    ]) {
        emit_sink_input_mute(&app, index, muted);
    }
}

#[tauri::command]
fn set_sink_volume(app: AppHandle, name: String, pct: u32) {
    if pactl_check(&["set-sink-volume", &name, &format!("{}%", pct)]) {
        emit_sink_volume(&app, &name, pct);
    }
}

#[tauri::command]
fn set_sink_mute(app: AppHandle, name: String, muted: bool) {
    if pactl_check(&["set-sink-mute", &name, if muted { "1" } else { "0" }]) {
        emit_sink_mute(&app, &name, muted);
    }
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
fn set_source_volume(app: AppHandle, name: String, pct: u32) {
    if pactl_check(&["set-source-volume", &name, &format!("{}%", pct)]) {
        emit_source_volume(&app, &name, pct);
    }
}

#[tauri::command]
fn set_source_mute(app: AppHandle, name: String, muted: bool) {
    if pactl_check(&["set-source-mute", &name, if muted { "1" } else { "0" }]) {
        emit_source_mute(&app, &name, muted);
    }
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

fn parse_pactl_volume_pct(item: &serde_json::Value) -> u32 {
    item["volume"]
        .as_object()
        .and_then(|m| m.values().next())
        .and_then(|v| v["value_percent"].as_str())
        .and_then(|s| s.trim_end_matches('%').parse::<u32>().ok())
        .unwrap_or(100)
}

fn snapshot_sources_state() -> HashMap<String, (u32, bool)> {
    let raw = pactl_output(&["-f", "json", "list", "sources"]);
    let json: serde_json::Value = serde_json::from_str(&raw).unwrap_or_default();
    let mut out = HashMap::new();
    for item in json.as_array().unwrap_or(&vec![]) {
        if let Some(name) = item["name"].as_str() {
            let muted = item["mute"].as_bool().unwrap_or(false);
            let volume = parse_pactl_volume_pct(item);
            out.insert(name.to_string(), (volume, muted));
        }
    }
    out
}

fn snapshot_sinks_state() -> HashMap<String, (u32, bool)> {
    let raw = pactl_output(&["-f", "json", "list", "sinks"]);
    let json: serde_json::Value = serde_json::from_str(&raw).unwrap_or_default();
    let mut out = HashMap::new();
    for item in json.as_array().unwrap_or(&vec![]) {
        if let Some(name) = item["name"].as_str() {
            let muted = item["mute"].as_bool().unwrap_or(false);
            let volume = parse_pactl_volume_pct(item);
            out.insert(name.to_string(), (volume, muted));
        }
    }
    out
}

fn spawn_audio_state_watcher(app: AppHandle) {
    use std::io::{BufRead, BufReader};
    use std::process::Stdio;
    thread::spawn(move || {
        let mut last_sources = snapshot_sources_state();
        let mut last_sinks = snapshot_sinks_state();
        loop {
            let child = match Command::new("pactl")
                .arg("subscribe")
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .spawn()
            {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("pactl subscribe spawn failed: {e}");
                    thread::sleep(Duration::from_secs(5));
                    continue;
                }
            };
            let stdout = match child.stdout {
                Some(s) => s,
                None => {
                    thread::sleep(Duration::from_secs(5));
                    continue;
                }
            };
            let reader = BufReader::new(stdout);
            for line in reader.lines().map_while(Result::ok) {
                if line.contains("on source") || line.contains("on sink") {
                    let now_sources = snapshot_sources_state();
                    for (name, &(vol, muted)) in &now_sources {
                        match last_sources.get(name) {
                            Some(&(prev_vol, prev_muted)) => {
                                if prev_muted != muted {
                                    emit_source_mute(&app, name, muted);
                                }
                                if prev_vol != vol {
                                    emit_source_volume(&app, name, vol);
                                }
                            }
                            None => {
                                emit_source_mute(&app, name, muted);
                                emit_source_volume(&app, name, vol);
                            }
                        }
                    }
                    last_sources = now_sources;

                    let now_sinks = snapshot_sinks_state();
                    for (name, &(vol, muted)) in &now_sinks {
                        match last_sinks.get(name) {
                            Some(&(prev_vol, prev_muted)) => {
                                if prev_muted != muted {
                                    emit_sink_mute(&app, name, muted);
                                }
                                if prev_vol != vol {
                                    emit_sink_volume(&app, name, vol);
                                }
                            }
                            None => {
                                emit_sink_mute(&app, name, muted);
                                emit_sink_volume(&app, name, vol);
                            }
                        }
                    }
                    last_sinks = now_sinks;
                }
            }
            thread::sleep(Duration::from_secs(2));
        }
    });
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
        dirs.push(PathBuf::from(format!(
            "{home}/.local/share/flatpak/exports/share/applications"
        )));
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
        if !in_main || trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if let Some((k, v)) = trimmed.split_once('=') {
            out.insert(k.trim().to_string(), v.trim().to_string());
        }
    }
    out
}

fn binary_name_from_command(cmd: &str) -> Option<String> {
    let first = cmd
        .split_whitespace()
        .find(|t| !t.starts_with('%') && !t.starts_with("env="))?;
    let path = std::path::Path::new(first);
    Some(path.file_name()?.to_string_lossy().to_string())
}

fn strip_common_suffixes(name: &str) -> String {
    let lower = name.to_ascii_lowercase();
    for suffix in [
        "-stable",
        "-bin",
        "-beta",
        "-nightly",
        "-dev",
        "-canary",
        "-unstable",
    ] {
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
            if path.extension().and_then(|s| s.to_str()) != Some("desktop") {
                continue;
            }
            let content = match fs::read_to_string(&path) {
                Ok(c) => c,
                Err(_) => continue,
            };
            let de = parse_desktop_entry(&content);
            if de.get("NoDisplay").map(|v| v == "true").unwrap_or(false) {
                continue;
            }
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
            let stem_last = stem_lower
                .rsplit('.')
                .next()
                .unwrap_or(&stem_lower)
                .to_string();
            let stem_stripped = strip_common_suffixes(&stem_last);
            let wm_class = de
                .get("StartupWMClass")
                .map(|s| s.to_ascii_lowercase())
                .unwrap_or_default();

            let priority: Option<u32> = if bin_lower == needle || stem_last == needle {
                Some(0)
            } else if wm_class == needle {
                Some(1)
            } else if bin_stripped == needle_stripped || stem_stripped == needle_stripped {
                Some(2)
            } else if needle.len() >= 3
                && (stem_last.starts_with(&needle)
                    || stem_last.ends_with(&needle)
                    || bin_lower.starts_with(&needle)
                    || bin_lower.ends_with(&needle))
            {
                Some(3)
            } else if needle.len() >= 4
                && (stem_last.contains(&needle)
                    || bin_lower.contains(&needle)
                    || needle.contains(&stem_last)
                    || needle.contains(&bin_lower))
            {
                Some(4)
            } else {
                None
            };

            if let Some(p) = priority {
                candidates.push((p, icon));
                if p == 0 {
                    break;
                }
            }
        }
        if candidates.iter().any(|(p, _)| *p == 0) {
            break;
        }
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
        if bin.is_empty() {
            out.insert(bin, None);
            continue;
        }
        let resolved = find_icon_name_for_binary(&bin).and_then(|n| resolve_icon_to_data_url(&n));
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
fn set_card_control_volume(app: AppHandle, card: u32, name: String, pct: u32) {
    let ok = Command::new("amixer")
        .args([
            "-c",
            &card.to_string(),
            "-M",
            "sset",
            &name,
            &format!("{}%", pct),
        ])
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if ok {
        emit_card_control_volume(&app, card, &name, pct);
    }
}

#[tauri::command]
fn set_card_control_mute(app: AppHandle, card: u32, name: String, muted: bool) {
    let ok = Command::new("amixer")
        .args([
            "-c",
            &card.to_string(),
            "sset",
            &name,
            if muted { "off" } else { "on" },
        ])
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if ok {
        let _ = app.emit(
            "tideline:card_control_mute_changed",
            serde_json::json!({ "card": card, "name": name, "muted": muted }),
        );
    }
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
    apply_mix_enabled(&app, &map, &cfg);
    *state.mix_enabled.lock().unwrap() = map;
    refresh_tray_menu(&app);
}

fn parse_accelerator(accel: &str) -> Result<Shortcut, String> {
    accel
        .parse::<Shortcut>()
        .map_err(|e| format!("invalid shortcut '{}': {}", accel, e))
}

fn channel_mute(app: &AppHandle, channel: &str) {
    let cfg = app.state::<AppState>().config.lock().unwrap().clone();
    let Some(ch) = cfg.channels.iter().find(|c| c.name == channel) else {
        return;
    };
    if ch.kind == ChannelKind::PhysicalInput {
        if ch.physical_source.is_empty() {
            return;
        }
        let cur = get_source_state(ch.physical_source.clone())
            .map(|(_, m)| m)
            .unwrap_or(false);
        let next = !cur;
        if pactl_check(&[
            "set-source-mute",
            &ch.physical_source,
            if next { "1" } else { "0" },
        ]) {
            emit_source_mute(app, &ch.physical_source, next);
        }
    } else {
        let sink = sink_node_for_channel(ch);
        let cur = get_sink_state(sink.clone())
            .map(|(_, m)| m)
            .unwrap_or(false);
        let next = !cur;
        if pactl_check(&["set-sink-mute", &sink, if next { "1" } else { "0" }]) {
            emit_sink_mute(app, &sink, next);
        }
    }
}

fn output_mute(app: &AppHandle, sink: &str) {
    let cur = get_sink_state(sink.to_string())
        .map(|(_, m)| m)
        .unwrap_or(false);
    let next = !cur;
    if pactl_check(&["set-sink-mute", sink, if next { "1" } else { "0" }]) {
        emit_sink_mute(app, sink, next);
    }
}

fn execute_keybind(app: &AppHandle, action: &KeybindAction) {
    match action {
        KeybindAction::ToggleOutputMute { sink } => output_mute(app, sink),
        KeybindAction::ToggleChannelMute { channel } => channel_mute(app, channel),
        KeybindAction::ToggleMixEnabled { mix_id } => {
            let state = app.state::<AppState>();
            let cfg = state.config.lock().unwrap().clone();
            let mut map = mix_enabled_with_defaults(&state.mix_enabled.lock().unwrap(), &cfg.mixes);
            let next = !*map.get(mix_id).unwrap_or(&true);
            map.insert(mix_id.clone(), next);
            write_mix_enabled(&map);
            apply_mix_enabled(app, &map, &cfg);
            *state.mix_enabled.lock().unwrap() = map;
            refresh_tray_menu(app);
        }
        KeybindAction::Plugin {
            plugin_id,
            action_id,
        } => {
            let registry = app
                .state::<Arc<tideline_host::PluginRegistry>>()
                .inner()
                .clone();
            let plugin_id = plugin_id.clone();
            let action_id = action_id.clone();
            tauri::async_runtime::spawn(async move {
                if let Err(e) =
                    plugins::dispatch_plugin_keybind(&registry, &plugin_id, &action_id).await
                {
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
fn set_keybind(
    accelerator: String,
    action: KeybindAction,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
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
fn clear_keybind(
    accelerator: String,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
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
async fn save_config(
    config: AppConfig,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<String, String> {
    save_config_to_disk(&config)?;

    let registry = app
        .state::<Arc<tideline_host::PluginRegistry>>()
        .inner()
        .clone();

    let old = state.config.lock().unwrap().clone();

    let msg = if let Some(added) = try_soft_apply(&old, &config) {
        let mix_mutes = build_mix_mutes(&config);
        let contributions: Vec<Vec<tideline_core::pipewire::directive::PipewireDirective>> =
            tideline_host::contribute::resolve_collisions(
                tideline_host::contribute::collect_pipewire_contributions(
                    &registry, &config, &mix_mutes,
                )
                .await,
            )
            .into_iter()
            .map(|c| c.directives)
            .collect();
        let backed_up = write_pipewire_conf_with_contributions(
            &config,
            &contributions,
            &mix_mutes,
            available_sink_names().as_ref(),
        )?;
        let mut soft_failed: Option<String> = None;
        for ch in &added {
            if let Err(e) = load_channel_modules(&config, ch) {
                soft_failed = Some(e);
                break;
            }
        }
        if soft_failed.is_none() {
            let added_names: HashSet<&str> = added.iter().map(|c| c.name.as_str()).collect();
            for ch in &config.channels {
                if ch.kind != ChannelKind::PhysicalInput {
                    continue;
                }
                if added_names.contains(ch.name.as_str()) {
                    continue;
                }
                if let Err(e) = load_channel_modules(&config, ch) {
                    soft_failed = Some(e);
                    break;
                }
            }
        }
        if let Some(err) = soft_failed {
            eprintln!("soft apply failed ({}); falling back to restart", err);
            restart_pipewire_stack(&registry).await;
            let mut m = String::from("Applied. Audio engine restarted (soft apply failed).");
            if !backed_up.is_empty() {
                m.push_str(&format!(
                    " Legacy files backed up: {}",
                    backed_up.join(", ")
                ));
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
                m.push_str(&format!(
                    " Legacy files backed up: {}",
                    backed_up.join(", ")
                ));
            }
            m
        }
    } else {
        write_pipewire_and_restart(&config, &registry).await?
    };

    *state.config.lock().unwrap() = config.clone();
    reapply_mix_enabled_state(&app, &state);
    refresh_tray_menu(&app);
    register_all_keybinds(&app);
    Ok(msg)
}

#[tauri::command]
fn save_config_quiet(
    config: AppConfig,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    // "Quiet" path for low-risk edits (add/remove a program on a channel,
    // tweak keybinds). Persists the new config and refreshes the pulse.rules
    // file used as a startup hint for newly-created streams, but does NOT
    // restart any audio services. The `routing::spawn` polling loop reads
    // AppState every 500ms and uses `pactl move-sink-input` to move existing
    // sink-inputs onto the correct channel sink, so a restart is not needed
    // to make routing changes take effect.
    //
    // Previously this function called `systemctl --user restart wireplumber`,
    // which (a) was the wrong service — pulse.rules is read by pipewire-pulse,
    // not wireplumber, (b) skipped the safety dance in `restart_pipewire_stack`
    // (mute snapshot/restore, host:pipewire_restarting plugin notification,
    // 800ms settle), wiping source mutes and severing the in-process LV2
    // host's pipewire links with no chance to re-attach, and (c) was redundant
    // with the polling loop. The result was that every add/remove of an app
    // on a channel broke audio.
    save_config_to_disk(&config)?;
    write_app_routing(&config)?;
    *state.config.lock().unwrap() = config;
    register_all_keybinds(&app);
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
fn set_channel_hidden(
    app: AppHandle,
    name: String,
    hidden: bool,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let mut cfg = state.config.lock().unwrap().clone();
    let mut found = false;
    for c in cfg.channels.iter_mut() {
        if c.name == name {
            c.hidden = hidden;
            found = true;
            break;
        }
    }
    if !found {
        return Err(format!("channel '{}' not found", name));
    }
    save_config_to_disk(&cfg)?;
    *state.config.lock().unwrap() = cfg;
    refresh_tray_menu(&app);
    Ok(())
}

#[tauri::command]
fn set_sink_hidden(
    app: AppHandle,
    name: String,
    hidden: bool,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let mut cfg = state.config.lock().unwrap().clone();
    let already = cfg.hidden_sinks.iter().any(|s| s == &name);
    if hidden && !already {
        cfg.hidden_sinks.push(name);
    } else if !hidden && already {
        cfg.hidden_sinks.retain(|s| s != &name);
    } else {
        return Ok(());
    }
    save_config_to_disk(&cfg)?;
    *state.config.lock().unwrap() = cfg;
    refresh_tray_menu(&app);
    Ok(())
}

#[tauri::command]
fn get_hidden_sinks(state: State<'_, AppState>) -> Vec<String> {
    state.config.lock().unwrap().hidden_sinks.clone()
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
    fetch_sink_inputs()
        .into_iter()
        .map(|s| s.node_name)
        .collect()
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

#[tauri::command]
fn open_plugin_window(
    app: AppHandle,
    plugin_id: String,
    surface_id: String,
    title: Option<String>,
    width: Option<f64>,
    height: Option<f64>,
) -> Result<(), String> {
    let label = format!("plugin-{}-{}", plugin_id, surface_id);
    if let Some(existing) = app.get_webview_window(&label) {
        let _ = existing.show();
        let _ = existing.set_focus();
        return Ok(());
    }
    let url_str = format!("tideline-plugin://{}/{}/", plugin_id, surface_id);
    let parsed = tauri::Url::parse(&url_str).map_err(|e| e.to_string())?;
    let win_title = title.unwrap_or_else(|| format!("{} – {}", plugin_id, surface_id));
    tauri::WebviewWindowBuilder::new(&app, &label, tauri::WebviewUrl::CustomProtocol(parsed))
        .title(win_title)
        .inner_size(width.unwrap_or(800.0), height.unwrap_or(600.0))
        .build()
        .map_err(|e| e.to_string())?;
    Ok(())
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

    let hidden: std::collections::HashSet<&str> =
        cfg.hidden_sinks.iter().map(|s| s.as_str()).collect();
    let visible_sinks: Vec<SinkInfo> = fetch_sinks()
        .into_iter()
        .filter(|s| !hidden.contains(s.name.as_str()))
        .collect();
    if !visible_sinks.is_empty() {
        let header = MenuItem::with_id(app, "hdr_outputs", "Outputs", false, None::<&str>)?;
        menu.append(&header)?;
        for s in &visible_sinks {
            let item = CheckMenuItem::with_id(
                app,
                format!("sinkmute:{}", s.name),
                &s.description,
                true,
                !s.muted,
                None::<&str>,
            )?;
            menu.append(&item)?;
        }
        let sep = PredefinedMenuItem::separator(app)?;
        menu.append(&sep)?;
    }

    let visible_inputs: Vec<&ChannelCfg> = cfg
        .channels
        .iter()
        .filter(|c| {
            !c.hidden && (c.kind == ChannelKind::Input || c.kind == ChannelKind::PhysicalInput)
        })
        .collect();
    if !visible_inputs.is_empty() {
        let header = MenuItem::with_id(app, "hdr_inputs", "Inputs", false, None::<&str>)?;
        menu.append(&header)?;
        for ch in &visible_inputs {
            let muted = if ch.kind == ChannelKind::PhysicalInput {
                if ch.physical_source.is_empty() {
                    false
                } else {
                    get_source_state(ch.physical_source.clone())
                        .map(|(_, m)| m)
                        .unwrap_or(false)
                }
            } else {
                let sink = sink_node_for_channel(ch);
                get_sink_state(sink).map(|(_, m)| m).unwrap_or(false)
            };
            let item = CheckMenuItem::with_id(
                app,
                format!("chmute:{}", ch.name),
                &ch.name,
                true,
                !muted,
                None::<&str>,
            )?;
            menu.append(&item)?;
        }
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
    let _log_guard = tideline_sdk::logging::init("tideline");

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

    let builder = tauri::Builder::default()
        .register_uri_scheme_protocol("tideline-plugin", |ctx, req| {
            plugins::handle_request(ctx, req)
        })
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init());

    #[cfg(debug_assertions)]
    let builder = builder.plugin(tauri_plugin_pilot::init());

    builder
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
            spawn_audio_state_watcher(app.handle().clone());
            routing::spawn(app.handle().clone());
            register_all_keybinds(app.handle());

            let plugin_registry = Arc::new(tideline_host::PluginRegistry::new());
            {
                let registry = plugin_registry.clone();
                let app_for_backend = app.handle().clone();
                tauri::async_runtime::spawn(async move {
                    registry
                        .set_backend(std::sync::Arc::new(
                            plugins::backend::TauriHostBackend::new(app_for_backend),
                        ))
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
            plugins::spawn_plugin_events_relay(app.handle(), plugin_registry.clone());

            // When a plugin's pipewire-relevant state changes (e.g.
            // tideline-effects' rack updates), re-collect contributions and
            // rewrite the pipewire conf. Multiple call sites (event-bus
            // rack_changed, direct attach_channel_data) feed a single
            // debounced trigger so we never double-restart on rapid edits.
            let pw_trigger = PipewireRebuildTrigger::new();
            app.manage(pw_trigger.clone());
            {
                let trigger = pw_trigger.clone();
                let mut rx = plugin_registry.subscribe_plugin_events();
                tauri::async_runtime::spawn(async move {
                    loop {
                        match rx.recv().await {
                            Ok(event) => {
                                if event.topic != "tideline-effects:rack_changed" {
                                    continue;
                                }
                                eprintln!("[pipewire] rack_changed observed — poking rebuild trigger");
                                trigger.poke();
                            }
                            Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                            Err(_) => break,
                        }
                    }
                });
            }
            {
                let registry = plugin_registry.clone();
                let app_for_rebuild = app.handle().clone();
                let trigger = pw_trigger.clone();
                tauri::async_runtime::spawn(async move {
                    loop {
                        trigger.notify.notified().await;
                        // Debounce: wait briefly, then drain any further pokes
                        // queued during the wait into this single rebuild.
                        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
                        let cfg = match app_for_rebuild.try_state::<AppState>() {
                            Some(s) => s.config.lock().unwrap().clone(),
                            None => continue,
                        };
                        eprintln!("[pipewire] rebuild worker firing write_pipewire_and_restart");
                        match write_pipewire_and_restart(&cfg, &registry).await {
                            Ok(msg) => eprintln!("[pipewire] rebuild ok — {msg}"),
                            Err(e) => eprintln!("[pipewire] rebuild failed: {e}"),
                        }
                        // Sink-inputs come back up over the next second or
                        // two as apps reconnect; mute is per-sink-input so a
                        // single reapply may miss late arrivals. Hammer it
                        // a few times to catch them.
                        let app_for_mute = app_for_rebuild.clone();
                        tauri::async_runtime::spawn(async move {
                            for delay_ms in [400u64, 1200, 2500, 5000] {
                                tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;
                                reapply_all_channel_volumes_and_mutes(&app_for_mute);
                            }
                        });
                    }
                });
            }

            {
                let registry = plugin_registry.clone();
                let app_for_pw = app.handle().clone();
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

                    tokio::time::sleep(std::time::Duration::from_secs(2)).await;

                    let cfg = match app_for_pw.try_state::<AppState>() {
                        Some(s) => s.config.lock().unwrap().clone(),
                        None => {
                            eprintln!("plugins: AppState missing, skipping startup pipewire collect");
                            return;
                        }
                    };
                    match write_pipewire_and_restart(&cfg, &registry).await {
                        Ok(msg) => eprintln!("plugins: startup pipewire write -- {msg}"),
                        Err(e) => eprintln!("plugins: startup pipewire write failed: {e}"),
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
            let app_for_vols = app.handle().clone();
            thread::spawn(move || {
                thread::sleep(Duration::from_millis(900));
                let all_vols = read_all_volumes();
                let mix_ids: Vec<String> = cfg_for_vols.mixes.iter().map(|m| m.id.clone()).collect();
                for ch in &cfg_for_vols.channels {
                    if let Some(vols) = all_vols.get(&ch.name) {
                        apply_channel_volumes(&app_for_vols, &slug(&ch.name), vols, &mix_ids);
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
                            apply_mix_enabled(app, &map, &cfg);
                            *state.mix_enabled.lock().unwrap() = map;
                            refresh_tray_menu(app);
                        }
                        s if s.starts_with("chmute:") => {
                            let name = s.trim_start_matches("chmute:").to_string();
                            channel_mute(app, &name);
                            refresh_tray_menu(app);
                        }
                        s if s.starts_with("sinkmute:") => {
                            let sink = s.trim_start_matches("sinkmute:").to_string();
                            output_mute(app, &sink);
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
            set_channel_hidden,
            set_sink_hidden,
            get_hidden_sinks,
            list_sinks,
            list_sink_input_nodes,
            window_minimize,
            window_hide,
            window_drag,
            open_plugin_window,
            ensure_audio_backend_cmd,
            get_initial_backend_status,
            get_all_channel_volumes,
            set_channel_master_volume,
            set_channel_mix_volume,
            set_channel_master_mute,
            set_channel_mix_mute,
            plugins::tideline_plugin_iframe_send,
            plugins::tideline_plugin_contributions,
            plugins::tideline_plugin_emit_event,
            plugins::tideline_plugin_request,
            plugins::tideline_plugin_replay_states,
            plugins::tideline_plugin_request_permission,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
