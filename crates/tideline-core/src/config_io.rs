use crate::model::{AppConfig, ChannelCfg, ChannelKind, Mix};
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
        plugin_data: HashMap::new(),
        hidden_sinks: Vec::new(),
    }
}

pub fn migrate_config(cfg: &mut AppConfig, raw: &str) {
    if !cfg.mixes.is_empty() {
        return;
    }
    let val: serde_json::Value = match serde_json::from_str(raw) {
        Ok(v) => v,
        Err(_) => return,
    };
    let hp = val
        .get("headphone_sink")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let sp = val
        .get("speaker_sink")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let mut hp_mix = Mix::new("headphones", "Headphones");
    if !hp.is_empty() {
        hp_mix.sinks = vec![hp];
    }
    let mut sp_mix = Mix::new("speakers", "Speakers");
    if !sp.is_empty() {
        sp_mix.sinks = vec![sp];
    }
    cfg.mixes = vec![hp_mix, sp_mix];
}

fn migrate_legacy_tones_into_plugin_data(raw: &mut serde_json::Value) {
    let Some(ptt) = raw.get_mut("ptt").and_then(|v| v.as_object_mut()) else {
        return;
    };
    let enabled = ptt
        .remove("tones_enabled")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);
    let volume = ptt
        .remove("tones_volume")
        .and_then(|v| v.as_u64())
        .unwrap_or(100) as u32;
    let plugin_data = raw
        .as_object_mut()
        .expect("AppConfig is a JSON object")
        .entry("plugin_data".to_string())
        .or_insert_with(|| serde_json::json!({}));
    let pd_obj = plugin_data
        .as_object_mut()
        .expect("plugin_data is an object");
    pd_obj
        .entry("tideline-tones".to_string())
        .or_insert_with(|| serde_json::json!({ "enabled": enabled, "volume": volume }));
}

fn migrate_legacy_ptt_into_plugin_data(raw: &mut serde_json::Value) {
    let Some(root) = raw.as_object_mut() else {
        return;
    };
    let Some(ptt) = root.remove("ptt") else {
        return;
    };
    if !ptt.is_object() {
        root.insert("ptt".to_string(), ptt);
        return;
    }
    let plugin_data = root
        .entry("plugin_data".to_string())
        .or_insert_with(|| serde_json::json!({}));
    let pd_obj = plugin_data
        .as_object_mut()
        .expect("plugin_data is an object");
    pd_obj.entry("tideline-ptt".to_string()).or_insert(ptt);
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
    let mut json: serde_json::Value =
        serde_json::from_str(&raw).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    migrate_legacy_tones_into_plugin_data(&mut json);
    migrate_legacy_ptt_into_plugin_data(&mut json);
    let mut cfg: AppConfig =
        serde_json::from_value(json).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
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

#[cfg(test)]
mod tones_migration_tests {
    use super::migrate_legacy_tones_into_plugin_data;
    use serde_json::json;

    #[test]
    fn legacy_tones_fields_move_into_plugin_data() {
        let mut raw = json!({
            "ptt": {
                "input_device": "mic",
                "tones_enabled": false,
                "tones_volume": 60
            }
        });
        migrate_legacy_tones_into_plugin_data(&mut raw);
        let ptt = raw.get("ptt").unwrap().as_object().unwrap();
        assert!(!ptt.contains_key("tones_enabled"));
        assert!(!ptt.contains_key("tones_volume"));
        let tones = raw
            .pointer("/plugin_data/tideline-tones")
            .expect("tones plugin data");
        assert_eq!(tones["enabled"], json!(false));
        assert_eq!(tones["volume"], json!(60));
    }

    #[test]
    fn missing_legacy_fields_use_defaults() {
        let mut raw = json!({
            "ptt": { "input_device": "mic" }
        });
        migrate_legacy_tones_into_plugin_data(&mut raw);
        let tones = raw
            .pointer("/plugin_data/tideline-tones")
            .expect("tones plugin data");
        assert_eq!(tones["enabled"], json!(true));
        assert_eq!(tones["volume"], json!(100));
    }

    #[test]
    fn does_not_overwrite_existing_plugin_data() {
        let mut raw = json!({
            "ptt": {
                "tones_enabled": false,
                "tones_volume": 60
            },
            "plugin_data": {
                "tideline-tones": { "enabled": true, "volume": 75 }
            }
        });
        migrate_legacy_tones_into_plugin_data(&mut raw);
        let ptt = raw.get("ptt").unwrap().as_object().unwrap();
        assert!(!ptt.contains_key("tones_enabled"));
        assert!(!ptt.contains_key("tones_volume"));
        let tones = raw
            .pointer("/plugin_data/tideline-tones")
            .expect("tones plugin data");
        assert_eq!(tones["enabled"], json!(true));
        assert_eq!(tones["volume"], json!(75));
    }
}

#[cfg(test)]
mod ptt_migration_tests {
    use super::migrate_legacy_ptt_into_plugin_data;
    use serde_json::json;

    #[test]
    fn legacy_ptt_object_moves_to_plugin_data() {
        let mut raw = json!({
            "ptt": {
                "mode": "ptt",
                "input_device": "alsa_input.usb",
                "mode_toggle_binding": "Ctrl+Shift+F12",
                "hold_binding": "Mouse5"
            },
            "channels": []
        });
        migrate_legacy_ptt_into_plugin_data(&mut raw);
        assert!(raw.get("ptt").is_none(), "ptt removed from root");
        let pd = raw
            .pointer("/plugin_data/tideline-ptt")
            .expect("ptt plugin data");
        assert_eq!(pd["mode"], json!("ptt"));
        assert_eq!(pd["input_device"], json!("alsa_input.usb"));
        assert_eq!(pd["hold_binding"], json!("Mouse5"));
    }

    #[test]
    fn missing_ptt_is_noop() {
        let mut raw = json!({ "channels": [] });
        migrate_legacy_ptt_into_plugin_data(&mut raw);
        assert!(raw.get("ptt").is_none());
        assert!(raw.get("plugin_data").is_none());
    }

    #[test]
    fn does_not_overwrite_existing_plugin_data() {
        let mut raw = json!({
            "ptt": { "mode": "ptt" },
            "plugin_data": {
                "tideline-ptt": { "mode": "open", "manually_set": true }
            }
        });
        migrate_legacy_ptt_into_plugin_data(&mut raw);
        let pd = raw.pointer("/plugin_data/tideline-ptt").unwrap();
        assert_eq!(pd["mode"], json!("open"));
        assert_eq!(pd["manually_set"], json!(true));
    }

    #[test]
    fn non_object_ptt_value_is_left_in_place() {
        let mut raw = json!({ "ptt": "garbage" });
        migrate_legacy_ptt_into_plugin_data(&mut raw);
        assert_eq!(raw["ptt"], json!("garbage"));
        assert!(raw.get("plugin_data").is_none());
    }

    #[test]
    fn runs_after_tones_migration() {
        use super::migrate_legacy_tones_into_plugin_data;
        let mut raw = json!({
            "ptt": {
                "mode": "ptt",
                "tones_enabled": false,
                "tones_volume": 60,
                "input_device": "mic"
            }
        });
        migrate_legacy_tones_into_plugin_data(&mut raw);
        migrate_legacy_ptt_into_plugin_data(&mut raw);

        assert_eq!(
            raw.pointer("/plugin_data/tideline-tones/enabled"),
            Some(&json!(false))
        );
        let ptt = raw.pointer("/plugin_data/tideline-ptt").unwrap();
        assert_eq!(ptt["mode"], json!("ptt"));
        assert_eq!(ptt["input_device"], json!("mic"));
        assert!(ptt.get("tones_enabled").is_none());
        assert!(ptt.get("tones_volume").is_none());
        assert!(raw.get("ptt").is_none());
    }
}
