use crate::ptt::binding::{Binding, Modifier};
use crate::ptt::PttRuntime;
use evdev::{Device, EventType, KeyCode};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tauri::AppHandle;

const MODIFIER_KEYS: &[(KeyCode, Modifier)] = &[
    (KeyCode::KEY_LEFTCTRL,  Modifier::Ctrl),
    (KeyCode::KEY_RIGHTCTRL, Modifier::Ctrl),
    (KeyCode::KEY_LEFTSHIFT, Modifier::Shift),
    (KeyCode::KEY_RIGHTSHIFT,Modifier::Shift),
    (KeyCode::KEY_LEFTALT,   Modifier::Alt),
    (KeyCode::KEY_RIGHTALT,  Modifier::Alt),
    (KeyCode::KEY_LEFTMETA,  Modifier::Super),
    (KeyCode::KEY_RIGHTMETA, Modifier::Super),
];

pub fn enumerate_input_devices() -> Vec<PathBuf> {
    let dir = Path::new("/dev/input");
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return Vec::new(),
    };
    let mut paths: Vec<PathBuf> = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|s| s.to_str()) else { continue };
        if !name.starts_with("event") { continue; }
        let Ok(dev) = Device::open(&path) else { continue };
        if dev.supported_events().contains(EventType::KEY) {
            paths.push(path);
        }
    }
    paths
}

/// Returns Err with an actionable message if /dev/input is unreadable
/// (most common cause: user not in `input` group).
pub fn probe_permission() -> Result<(), String> {
    let dir = Path::new("/dev/input");
    let Ok(mut entries) = std::fs::read_dir(dir) else {
        return Err(format!(
            "Cannot read /dev/input. Add yourself to the input group, then log out and back in:\n\n  sudo usermod -aG input $USER"
        ));
    };
    // Try to actually open one event device to detect EACCES vs ENOENT
    while let Some(Ok(entry)) = entries.next() {
        let path = entry.path();
        let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
        if name.starts_with("event") {
            return Device::open(&path)
                .map(|_| ())
                .map_err(|e| format!(
                    "Cannot open {}: {}. Add yourself to the input group, then log out and back in:\n\n  sudo usermod -aG input $USER",
                    path.display(), e
                ));
        }
    }
    Ok(())
}

#[allow(dead_code)]
pub(crate) fn modifier_for_key(k: KeyCode) -> Option<Modifier> {
    MODIFIER_KEYS.iter().find(|(key, _)| *key == k).map(|(_, m)| *m)
}
