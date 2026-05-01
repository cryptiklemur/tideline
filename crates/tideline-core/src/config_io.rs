use crate::model::{AppConfig, ChannelCfg, ChannelKind, Mix, PttConfig};
use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

const BACKUP_SUFFIX: &str = ".pre-uuid.bak";

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
    let mut ch = ChannelCfg::new(name);
    ch.kind = ChannelKind::Output;
    ch.hp_node = format!("playback.{}-hp", s);
    ch.sp_node = format!("playback.{}-sp", s);
    ch
}

pub fn default_config() -> AppConfig {
    AppConfig {
        mixes: vec![
            Mix::new("headphones", "Headphones"),
            Mix::new("speakers", "Speakers"),
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
    let mut hp_mix = Mix::new("headphones", "Headphones");
    if !hp.is_empty() { hp_mix.sinks = vec![hp]; }
    let mut sp_mix = Mix::new("speakers", "Speakers");
    if !sp.is_empty() { sp_mix.sinks = vec![sp]; }
    cfg.mixes = vec![hp_mix, sp_mix];
}

pub fn load_config_from(path: &Path) -> io::Result<AppConfig> {
    let raw = fs::read_to_string(path)?;
    let needs_backup = !raw.contains("\"uuid\"");
    if needs_backup {
        let backup_path = backup_path_for(path);
        if !backup_path.exists() {
            fs::write(&backup_path, &raw)?;
        }
    }
    let mut cfg: AppConfig = serde_json::from_str(&raw)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    migrate_config(&mut cfg, &raw);
    Ok(cfg)
}

pub fn save_config_to(path: &Path, cfg: &AppConfig) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(cfg)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    fs::write(path, json)
}

fn backup_path_for(path: &Path) -> PathBuf {
    let mut s = path.as_os_str().to_owned();
    s.push(BACKUP_SUFFIX);
    s.into()
}

pub fn load_config() -> AppConfig {
    let path = config_path();
    if let Ok(cfg) = load_config_from(&path) {
        return cfg;
    }
    let cfg = default_config();
    let _ = save_config_to_disk(&cfg);
    cfg
}

pub fn save_config_to_disk(cfg: &AppConfig) -> Result<(), String> {
    save_config_to(&config_path(), cfg).map_err(|e| e.to_string())
}
