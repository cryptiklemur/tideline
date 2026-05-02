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
        plugin_data: HashMap::new(),
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

fn migrate_legacy_tones_into_plugin_data(raw: &mut serde_json::Value) {
    let Some(ptt) = raw.get_mut("ptt").and_then(|v| v.as_object_mut()) else { return; };
    let enabled = ptt.remove("tones_enabled").and_then(|v| v.as_bool()).unwrap_or(true);
    let volume  = ptt.remove("tones_volume").and_then(|v| v.as_u64()).unwrap_or(100) as u32;
    let plugin_data = raw
        .as_object_mut()
        .expect("AppConfig is a JSON object")
        .entry("plugin_data".to_string())
        .or_insert_with(|| serde_json::json!({}));
    let pd_obj = plugin_data.as_object_mut().expect("plugin_data is an object");
    pd_obj.entry("tideline-tones".to_string()).or_insert_with(|| {
        serde_json::json!({ "enabled": enabled, "volume": volume })
    });
}

fn migrate_legacy_led_enabled_into_plugin_data(raw: &mut serde_json::Value) {
    let Some(ptt) = raw.get("ptt").and_then(|v| v.as_object()) else { return; };
    let Some(led_enabled) = ptt.get("led_enabled").and_then(|v| v.as_bool()) else { return; };
    let plugin_data = raw
        .as_object_mut()
        .expect("AppConfig is a JSON object")
        .entry("plugin_data".to_string())
        .or_insert_with(|| serde_json::json!({}));
    let pd_obj = plugin_data.as_object_mut().expect("plugin_data is an object");
    pd_obj.entry("tideline-wave-xlr".to_string()).or_insert_with(|| {
        serde_json::json!({ "led_enabled": led_enabled })
    });
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
    let mut json: serde_json::Value = serde_json::from_str(&raw)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    migrate_legacy_tones_into_plugin_data(&mut json);
    migrate_legacy_led_enabled_into_plugin_data(&mut json);
    let mut cfg: AppConfig = serde_json::from_value(json)
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
mod led_enabled_migration_tests {
    use super::migrate_legacy_led_enabled_into_plugin_data;
    use serde_json::json;

    #[test]
    fn legacy_led_enabled_copied_into_plugin_data() {
        let mut raw = json!({
            "ptt": { "led_enabled": false }
        });
        migrate_legacy_led_enabled_into_plugin_data(&mut raw);
        let bag = raw
            .pointer("/plugin_data/tideline-wave-xlr")
            .expect("wave-xlr plugin data");
        assert_eq!(bag["led_enabled"], json!(false));
    }

    #[test]
    fn missing_led_enabled_field_is_a_noop() {
        let mut raw = json!({ "ptt": {} });
        migrate_legacy_led_enabled_into_plugin_data(&mut raw);
        assert!(raw.pointer("/plugin_data/tideline-wave-xlr").is_none());
    }

    #[test]
    fn does_not_overwrite_existing_plugin_data() {
        let mut raw = json!({
            "ptt": { "led_enabled": true },
            "plugin_data": {
                "tideline-wave-xlr": { "led_enabled": false }
            }
        });
        migrate_legacy_led_enabled_into_plugin_data(&mut raw);
        assert_eq!(
            raw["plugin_data"]["tideline-wave-xlr"]["led_enabled"],
            json!(false),
            "existing namespaced value must not be clobbered"
        );
    }

    #[test]
    fn creates_plugin_data_object_when_absent() {
        let mut raw = json!({
            "ptt": { "led_enabled": true }
        });
        migrate_legacy_led_enabled_into_plugin_data(&mut raw);
        assert_eq!(
            raw["plugin_data"]["tideline-wave-xlr"]["led_enabled"],
            json!(true)
        );
    }
}
