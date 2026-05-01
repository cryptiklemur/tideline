use crate::model::{AppConfig, ChannelCfg, ChannelKind, Mix, PttConfig};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

pub fn home_dir() -> PathBuf {
    PathBuf::from(std::env::var("HOME").unwrap_or_else(|_| "/root".into()))
}

pub fn config_path() -> PathBuf {
    home_dir().join(".config/tideline/config.json")
}

pub fn pipewire_conf_dir() -> PathBuf {
    home_dir().join(".config/pipewire/pipewire.conf.d")
}

pub fn wireplumber_conf_dir() -> PathBuf {
    home_dir().join(".config/wireplumber/wireplumber.conf.d")
}

pub fn pulse_conf_dir() -> PathBuf {
    home_dir().join(".config/pipewire/pipewire-pulse.conf.d")
}

pub fn slug(name: &str) -> String {
    name.to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '_' })
        .collect()
}

pub fn output_channel(name: &str) -> ChannelCfg {
    let s = slug(name);
    ChannelCfg {
        name: name.into(),
        kind: ChannelKind::Output,
        hp_node: format!("playback.{}-hp", s),
        sp_node: format!("playback.{}-sp", s),
        programs: Vec::new(),
        sources: Vec::new(),
        physical_source: String::new(),
        icon: String::new(),
    }
}

pub fn default_config() -> AppConfig {
    AppConfig {
        mixes: vec![
            Mix { id: "headphones".into(), name: "Headphones".into(), sinks: Vec::new() },
            Mix { id: "speakers".into(),   name: "Speakers".into(),   sinks: Vec::new() },
        ],
        channels: vec![
            output_channel("Browser"),
            output_channel("Music"),
            output_channel("Games"),
            output_channel("System"),
        ],
        keybinds: HashMap::new(),
        ptt: PttConfig::default(),
    }
}

pub fn migrate_config(cfg: &mut AppConfig, raw: &str) {
    if !cfg.mixes.is_empty() { return; }
    let val: serde_json::Value = match serde_json::from_str(raw) {
        Ok(v) => v,
        Err(_) => return,
    };
    let hp = val.get("headphone_sink").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let sp = val.get("speaker_sink").and_then(|v| v.as_str()).unwrap_or("").to_string();
    cfg.mixes = vec![
        Mix {
            id: "headphones".into(),
            name: "Headphones".into(),
            sinks: if hp.is_empty() { Vec::new() } else { vec![hp] },
        },
        Mix {
            id: "speakers".into(),
            name: "Speakers".into(),
            sinks: if sp.is_empty() { Vec::new() } else { vec![sp] },
        },
    ];
}

pub fn load_config() -> AppConfig {
    if let Ok(data) = fs::read_to_string(config_path()) {
        if let Ok(mut cfg) = serde_json::from_str::<AppConfig>(&data) {
            migrate_config(&mut cfg, &data);
            return cfg;
        }
    }
    let cfg = default_config();
    let _ = save_config_to_disk(&cfg);
    cfg
}

pub fn save_config_to_disk(cfg: &AppConfig) -> Result<(), String> {
    let path = config_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let json = serde_json::to_string_pretty(cfg).map_err(|e| e.to_string())?;
    fs::write(&path, json).map_err(|e| e.to_string())
}
