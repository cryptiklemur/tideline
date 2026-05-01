use std::fs;
use tempfile::tempdir;
use tideline_core::config_io::{load_config_from, save_config_to};

#[test]
fn loading_legacy_config_writes_backup_once() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("config.json");
    let backup = dir.path().join("config.json.pre-uuid.bak");

    let legacy = r#"{
        "mixes": [{"id": "default", "name": "Default", "sinks": []}],
        "channels": [],
        "keybinds": {},
        "ptt": {
            "mode": "open", "mode_toggle_binding": null, "hold_binding": null,
            "input_device": "", "tones_enabled": false, "tones_volume": 0, "led_enabled": false
        }
    }"#;
    fs::write(&path, legacy).unwrap();

    let cfg = load_config_from(&path).unwrap();
    save_config_to(&path, &cfg).unwrap();

    assert!(backup.exists(), "backup file must be written on first migration");
    let backup_contents = fs::read_to_string(&backup).unwrap();
    assert_eq!(backup_contents, legacy, "backup must be byte-identical to original");
}

#[test]
fn second_load_does_not_overwrite_existing_backup() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("config.json");
    let backup = dir.path().join("config.json.pre-uuid.bak");
    fs::write(&backup, "ORIGINAL").unwrap();

    let already_migrated = r#"{
        "mixes": [], "channels": [], "keybinds": {},
        "ptt": {
            "mode": "open", "mode_toggle_binding": null, "hold_binding": null,
            "input_device": "", "tones_enabled": false, "tones_volume": 0, "led_enabled": false
        }
    }"#;
    fs::write(&path, already_migrated).unwrap();
    let _ = load_config_from(&path).unwrap();

    assert_eq!(fs::read_to_string(&backup).unwrap(), "ORIGINAL");
}
