use crate::binding::{Binding, Modifier};
use crate::runtime::PttRuntime;
use evdev::{Device, EventSummary, EventType, KeyCode};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use tokio::runtime::Handle;

const MODIFIER_KEYS: &[(KeyCode, Modifier)] = &[
    (KeyCode::KEY_LEFTCTRL, Modifier::Ctrl),
    (KeyCode::KEY_RIGHTCTRL, Modifier::Ctrl),
    (KeyCode::KEY_LEFTSHIFT, Modifier::Shift),
    (KeyCode::KEY_RIGHTSHIFT, Modifier::Shift),
    (KeyCode::KEY_LEFTALT, Modifier::Alt),
    (KeyCode::KEY_RIGHTALT, Modifier::Alt),
    (KeyCode::KEY_LEFTMETA, Modifier::Super),
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
        let Some(name) = path.file_name().and_then(|s| s.to_str()) else {
            continue;
        };
        if !name.starts_with("event") {
            continue;
        }
        let Ok(dev) = Device::open(&path) else {
            continue;
        };
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
        return Err(
            "Cannot read /dev/input. Add yourself to the input group, then log out and back in:\n\n  sudo usermod -aG input $USER".to_string()
        );
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

pub(crate) fn modifier_for_key(k: KeyCode) -> Option<Modifier> {
    MODIFIER_KEYS
        .iter()
        .find(|(key, _)| *key == k)
        .map(|(_, m)| *m)
}

fn key_to_binding_key(k: KeyCode) -> Option<&'static str> {
    Some(match k {
        KeyCode::KEY_F1 => "F1",
        KeyCode::KEY_F2 => "F2",
        KeyCode::KEY_F3 => "F3",
        KeyCode::KEY_F4 => "F4",
        KeyCode::KEY_F5 => "F5",
        KeyCode::KEY_F6 => "F6",
        KeyCode::KEY_F7 => "F7",
        KeyCode::KEY_F8 => "F8",
        KeyCode::KEY_F9 => "F9",
        KeyCode::KEY_F10 => "F10",
        KeyCode::KEY_F11 => "F11",
        KeyCode::KEY_F12 => "F12",
        KeyCode::KEY_F13 => "F13",
        KeyCode::KEY_F14 => "F14",
        KeyCode::KEY_F15 => "F15",
        KeyCode::KEY_F16 => "F16",
        KeyCode::KEY_F17 => "F17",
        KeyCode::KEY_F18 => "F18",
        KeyCode::KEY_F19 => "F19",
        KeyCode::KEY_F20 => "F20",
        KeyCode::KEY_F21 => "F21",
        KeyCode::KEY_F22 => "F22",
        KeyCode::KEY_F23 => "F23",
        KeyCode::KEY_F24 => "F24",
        KeyCode::KEY_A => "A",
        KeyCode::KEY_B => "B",
        KeyCode::KEY_C => "C",
        KeyCode::KEY_D => "D",
        KeyCode::KEY_E => "E",
        KeyCode::KEY_F => "F",
        KeyCode::KEY_G => "G",
        KeyCode::KEY_H => "H",
        KeyCode::KEY_I => "I",
        KeyCode::KEY_J => "J",
        KeyCode::KEY_K => "K",
        KeyCode::KEY_L => "L",
        KeyCode::KEY_M => "M",
        KeyCode::KEY_N => "N",
        KeyCode::KEY_O => "O",
        KeyCode::KEY_P => "P",
        KeyCode::KEY_Q => "Q",
        KeyCode::KEY_R => "R",
        KeyCode::KEY_S => "S",
        KeyCode::KEY_T => "T",
        KeyCode::KEY_U => "U",
        KeyCode::KEY_V => "V",
        KeyCode::KEY_W => "W",
        KeyCode::KEY_X => "X",
        KeyCode::KEY_Y => "Y",
        KeyCode::KEY_Z => "Z",
        KeyCode::KEY_0 => "0",
        KeyCode::KEY_1 => "1",
        KeyCode::KEY_2 => "2",
        KeyCode::KEY_3 => "3",
        KeyCode::KEY_4 => "4",
        KeyCode::KEY_5 => "5",
        KeyCode::KEY_6 => "6",
        KeyCode::KEY_7 => "7",
        KeyCode::KEY_8 => "8",
        KeyCode::KEY_9 => "9",
        KeyCode::KEY_SPACE => "Space",
        KeyCode::KEY_ENTER => "Enter",
        KeyCode::KEY_TAB => "Tab",
        KeyCode::KEY_BACKSPACE => "Backspace",
        KeyCode::KEY_DELETE => "Delete",
        KeyCode::KEY_INSERT => "Insert",
        KeyCode::KEY_HOME => "Home",
        KeyCode::KEY_END => "End",
        KeyCode::KEY_PAGEUP => "PageUp",
        KeyCode::KEY_PAGEDOWN => "PageDown",
        KeyCode::KEY_UP => "Up",
        KeyCode::KEY_DOWN => "Down",
        KeyCode::KEY_LEFT => "Left",
        KeyCode::KEY_RIGHT => "Right",
        KeyCode::KEY_ESC => "Escape",
        KeyCode::KEY_CAPSLOCK => "CapsLock",
        KeyCode::KEY_NUMLOCK => "NumLock",
        KeyCode::KEY_SCROLLLOCK => "ScrollLock",
        KeyCode::KEY_SYSRQ => "PrintScreen",
        KeyCode::KEY_PAUSE => "Pause",
        KeyCode::KEY_MINUS => "Minus",
        KeyCode::KEY_EQUAL => "Equal",
        KeyCode::KEY_LEFTBRACE => "BracketLeft",
        KeyCode::KEY_RIGHTBRACE => "BracketRight",
        KeyCode::KEY_SEMICOLON => "Semicolon",
        KeyCode::KEY_APOSTROPHE => "Quote",
        KeyCode::KEY_COMMA => "Comma",
        KeyCode::KEY_DOT => "Period",
        KeyCode::KEY_SLASH => "Slash",
        KeyCode::KEY_BACKSLASH => "Backslash",
        KeyCode::KEY_GRAVE => "Backquote",
        _ => return None,
    })
}

fn key_to_mouse_token(k: KeyCode) -> Option<&'static str> {
    Some(match k {
        KeyCode::BTN_LEFT => "Mouse1",
        KeyCode::BTN_RIGHT => "Mouse2",
        KeyCode::BTN_MIDDLE => "Mouse3",
        KeyCode::BTN_SIDE => "Mouse4",
        KeyCode::BTN_EXTRA => "Mouse5",
        KeyCode::BTN_FORWARD => "Mouse6",
        KeyCode::BTN_BACK => "Mouse7",
        KeyCode::BTN_TASK => "Mouse8",
        _ => return None,
    })
}

/// Convert a fired `KeyCode` into a `Binding` given current modifier state.
/// Returns None if the key is purely a modifier or not in the supported set.
pub(crate) fn key_to_binding(k: KeyCode, mods: &HashSet<Modifier>) -> Option<Binding> {
    if MODIFIER_KEYS.iter().any(|(mk, _)| *mk == k) {
        return None;
    }
    let canonical_mods: Vec<Modifier> = {
        let mut v: Vec<Modifier> = mods.iter().copied().collect();
        v.sort_by_key(|m| match m {
            Modifier::Ctrl => 0,
            Modifier::Shift => 1,
            Modifier::Alt => 2,
            Modifier::Super => 3,
        });
        v
    };
    if let Some(button) = key_to_mouse_token(k) {
        return Some(Binding::Mouse {
            mods: canonical_mods,
            button: button.to_string(),
        });
    }
    if let Some(key) = key_to_binding_key(k) {
        return Some(Binding::Keyboard {
            mods: canonical_mods,
            key: key.to_string(),
        });
    }
    None
}

struct Shared {
    runtime: Arc<PttRuntime>,
    handle: Handle,
    mods: Mutex<HashSet<Modifier>>,
    /// Set of physical keys currently held (so we can debounce repeats and
    /// detect releases of the binding key). Stored as raw evdev codes.
    keys_down: Mutex<HashSet<u16>>,
}

pub async fn start(runtime: Arc<PttRuntime>) {
    if let Err(msg) = probe_permission() {
        runtime.set_error(Some(msg)).await;
    }
    let shared = Arc::new(Shared {
        runtime,
        handle: Handle::current(),
        mods: Mutex::new(HashSet::new()),
        keys_down: Mutex::new(HashSet::new()),
    });
    thread::spawn(move || rescan_loop(shared));
}

fn rescan_loop(shared: Arc<Shared>) {
    let mut active: HashSet<PathBuf> = HashSet::new();
    loop {
        let devices = enumerate_input_devices();
        for path in &devices {
            if active.insert(path.clone()) {
                let path_owned = path.clone();
                let shared = shared.clone();
                thread::spawn(move || device_loop(&path_owned, shared));
            }
        }
        active.retain(|p| p.exists());
        thread::sleep(Duration::from_secs(5));
    }
}

fn device_loop(path: &Path, shared: Arc<Shared>) {
    let Ok(mut dev) = Device::open(path) else {
        return;
    };
    loop {
        let events = match dev.fetch_events() {
            Ok(it) => it,
            Err(_) => return,
        };
        for ev in events {
            if let EventSummary::Key(_, k, value) = ev.destructure() {
                handle_key_event(&shared, k, value);
            }
        }
    }
}

fn handle_key_event(shared: &Shared, k: KeyCode, value: i32) {
    if value == 2 {
        return;
    }

    if let Some(m) = modifier_for_key(k) {
        let mut mods = shared.mods.lock().unwrap();
        if value == 1 {
            mods.insert(m);
        } else {
            mods.remove(&m);
        }
        return;
    }

    let raw_code = k.code();
    if value == 1 {
        let inserted = shared.keys_down.lock().unwrap().insert(raw_code);
        if !inserted {
            return;
        }
    } else {
        shared.keys_down.lock().unwrap().remove(&raw_code);
    }

    let mods_snapshot = { shared.mods.lock().unwrap().clone() };
    let Some(observed) = key_to_binding(k, &mods_snapshot) else {
        return;
    };

    let runtime = shared.runtime.clone();
    let captured_value = value;
    shared.handle.spawn(async move {
        let cfg = runtime.config.lock().await.clone();
        eprintln!(
            "PTT: evdev key event observed={:?} value={} enabled_sources={}",
            observed,
            captured_value,
            cfg.enabled_sources.len()
        );
        let toggle = cfg.mode_toggle_binding.clone();
        let hold = cfg.hold_binding.clone();
        let sources = cfg.enabled_sources.clone();
        drop(cfg);
        if captured_value == 1 {
            if let Some(t) = toggle.as_ref() {
                if *t == observed {
                    for src in &sources {
                        runtime.toggle_mode(src).await;
                    }
                    return;
                }
            }
            if let Some(h) = hold.as_ref() {
                if *h == observed {
                    for src in &sources {
                        runtime.hold_press(src).await;
                    }
                }
            }
        } else if captured_value == 0 {
            if let Some(h) = hold.as_ref() {
                let key_match = match (h, &observed) {
                    (Binding::Keyboard { key: hk, .. }, Binding::Keyboard { key: ok, .. }) => {
                        hk == ok
                    }
                    (Binding::Mouse { button: hb, .. }, Binding::Mouse { button: ob, .. }) => {
                        hb == ob
                    }
                    _ => false,
                };
                if key_match {
                    for src in &sources {
                        runtime.hold_release(src).await;
                    }
                }
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use evdev::KeyCode;
    use std::collections::HashSet;

    #[test]
    fn f12_with_ctrl_shift() {
        let mut mods = HashSet::new();
        mods.insert(Modifier::Shift);
        mods.insert(Modifier::Ctrl);
        let b = key_to_binding(KeyCode::KEY_F12, &mods).unwrap();
        assert_eq!(b.format(), "Ctrl+Shift+F12");
    }
    #[test]
    fn mouse5_no_mods() {
        let b = key_to_binding(KeyCode::BTN_EXTRA, &HashSet::new()).unwrap();
        assert_eq!(b.format(), "Mouse5");
    }
    #[test]
    fn pure_modifier_returns_none() {
        assert!(key_to_binding(KeyCode::KEY_LEFTCTRL, &HashSet::new()).is_none());
    }
}
