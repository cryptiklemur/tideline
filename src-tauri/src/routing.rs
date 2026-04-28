use std::collections::HashMap;
use std::process::Command;
use std::thread;
use std::time::Duration;

use crate::{slug, AppState, ChannelKind};
use tauri::{AppHandle, Manager};

pub fn spawn(app: AppHandle) {
    thread::spawn(move || loop {
        apply_once(&app);
        thread::sleep(Duration::from_millis(500));
    });
}

fn apply_once(app: &AppHandle) {
    let binary_to_sink = match build_routing_table(app) {
        Some(t) if !t.is_empty() => t,
        _ => return,
    };

    let inputs = pactl_json(&["-f", "json", "list", "sink-inputs"]);
    let clients = pactl_json(&["-f", "json", "list", "clients"]);
    let sinks = pactl_json(&["-f", "json", "list", "sinks"]);

    let client_bin = client_binary_map(&clients);
    let sink_idx_name = sink_index_name_map(&sinks);

    for input in inputs.as_array().unwrap_or(&Vec::new()) {
        let node_name = input["properties"]["node.name"].as_str().unwrap_or("");
        if node_name.starts_with("playback.") || node_name.starts_with("capture.") {
            continue;
        }
        let input_idx = match input["index"].as_u64() {
            Some(v) => v,
            None => continue,
        };
        let client_idx = match input["client"]
            .as_u64()
            .or_else(|| input["client"].as_str().and_then(|s| s.parse().ok()))
        {
            Some(v) => v,
            None => continue,
        };
        let cur_sink_idx = input["sink"].as_u64().unwrap_or(u64::MAX);
        let cur_sink_name = sink_idx_name.get(&cur_sink_idx).cloned().unwrap_or_default();

        let bin = match client_bin.get(&client_idx) {
            Some(b) => b,
            None => continue,
        };
        let target = match binary_to_sink.get(bin.as_str()) {
            Some(t) => t,
            None => continue,
        };
        if cur_sink_name == *target {
            continue;
        }
        let _ = Command::new("pactl")
            .args(["move-sink-input", &input_idx.to_string(), target])
            .status();
    }
}

fn build_routing_table(app: &AppHandle) -> Option<HashMap<String, String>> {
    let state = app.state::<AppState>();
    let cfg = state.config.lock().ok()?;
    let mut map = HashMap::new();
    for ch in &cfg.channels {
        if ch.kind != ChannelKind::Output {
            continue;
        }
        let target = format!("sink.{}", slug(&ch.name));
        for prog in &ch.programs {
            map.insert(prog.clone(), target.clone());
        }
    }
    Some(map)
}

fn pactl_json(args: &[&str]) -> serde_json::Value {
    Command::new("pactl")
        .args(args)
        .output()
        .ok()
        .and_then(|o| serde_json::from_slice::<serde_json::Value>(&o.stdout).ok())
        .unwrap_or_default()
}

fn client_binary_map(clients: &serde_json::Value) -> HashMap<u64, String> {
    let mut map = HashMap::new();
    for c in clients.as_array().unwrap_or(&Vec::new()) {
        let idx = c["index"].as_u64();
        let bin = c["properties"]["application.process.binary"].as_str();
        if let (Some(idx), Some(bin)) = (idx, bin) {
            map.insert(idx, bin.to_string());
        }
    }
    map
}

fn sink_index_name_map(sinks: &serde_json::Value) -> HashMap<u64, String> {
    let mut map = HashMap::new();
    for s in sinks.as_array().unwrap_or(&Vec::new()) {
        if let (Some(idx), Some(name)) = (s["index"].as_u64(), s["name"].as_str()) {
            map.insert(idx, name.to_string());
        }
    }
    map
}
