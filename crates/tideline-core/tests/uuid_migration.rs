use tideline_core::model::{AppConfig, ChannelCfg, Mix};

#[test]
fn legacy_config_without_uuid_deserializes_with_fresh_uuids() {
    let json = r#"{
        "mixes": [{"id": "default", "name": "Default", "sinks": []}],
        "channels": [{
            "name": "Game",
            "kind": "output",
            "hp_node": "",
            "sp_node": "",
            "programs": [],
            "sources": [],
            "physical_source": "",
            "icon": ""
        }],
        "keybinds": {},
        "ptt": {
            "mode": "open",
            "mode_toggle_binding": null,
            "hold_binding": null,
            "input_device": "",
            "tones_enabled": false,
            "tones_volume": 0,
            "led_enabled": false
        }
    }"#;
    let cfg: AppConfig = serde_json::from_str(json).unwrap();
    assert_eq!(cfg.channels.len(), 1);
    assert!(
        !cfg.channels[0].uuid.is_nil(),
        "uuid must be assigned on migration"
    );
    assert!(cfg.channels[0].plugin_data.is_empty());
    assert!(!cfg.mixes[0].uuid.is_nil());
    assert!(cfg.mixes[0].plugin_data.is_empty());
}

#[test]
fn channel_uuid_roundtrips_through_serde() {
    let mut cfg = AppConfig::default();
    cfg.channels.push(ChannelCfg::new("Game"));
    let json = serde_json::to_string(&cfg).unwrap();
    let back: AppConfig = serde_json::from_str(&json).unwrap();
    assert_eq!(cfg.channels[0].uuid, back.channels[0].uuid);
}

#[test]
fn mix_uuid_roundtrips_through_serde() {
    let mut cfg = AppConfig::default();
    cfg.mixes.push(Mix::new("default", "Default"));
    let json = serde_json::to_string(&cfg).unwrap();
    let back: AppConfig = serde_json::from_str(&json).unwrap();
    assert_eq!(cfg.mixes[0].uuid, back.mixes[0].uuid);
}
