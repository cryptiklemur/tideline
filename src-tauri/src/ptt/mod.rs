pub mod binding;
pub mod evdev_listener;
pub mod mute;
pub mod notify;
pub mod state;
pub mod tones;
pub mod wave_xlr;

use crate::{AppConfig, AppState, Mode};
use serde::Serialize;
use state::Effects;
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, Manager};

/// Runtime owned by Tauri's state, shared by the evdev listener and Tauri commands.
pub struct PttRuntime {
    pub state: Mutex<state::PttState>,
    pub last_error: Mutex<Option<String>>,
}

impl PttRuntime {
    pub fn from_config(cfg: &AppConfig) -> Arc<Self> {
        Arc::new(Self {
            state: Mutex::new(state::PttState::new(cfg.ptt.mode)),
            last_error: Mutex::new(None),
        })
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct PttStateEvent {
    pub mode: Mode,
    pub hold_active: bool,
    pub transmitting: bool,
    pub error: Option<String>,
}

fn emit_state(app: &AppHandle, runtime: &PttRuntime) {
    let s = *runtime.state.lock().unwrap();
    let err = runtime.last_error.lock().unwrap().clone();
    let _ = app.emit("ptt:state", PttStateEvent {
        mode: s.mode,
        hold_active: s.hold_active,
        transmitting: s.transmitting(),
        error: err,
    });
}

pub fn set_error(app: &AppHandle, runtime: &PttRuntime, msg: Option<String>) {
    *runtime.last_error.lock().unwrap() = msg;
    emit_state(app, runtime);
}

/// Apply an Effects record. Pulls config snapshot from AppState for tone
/// settings + input device. Persists mode by writing to AppState's config
/// and to disk via the existing save_config_to_disk path.
pub fn apply_effects(app: &AppHandle, runtime: &PttRuntime, fx: &Effects) {
    let cfg_snapshot: AppConfig = {
        let st = app.state::<AppState>();
        let cfg = st.config.lock().unwrap().clone();
        cfg
    };

    if let Some(muted) = fx.set_muted {
        if let Err(e) = mute::set_source_mute(&cfg_snapshot.ptt.input_device, muted) {
            *runtime.last_error.lock().unwrap() = Some(format!("mute failed: {}", e));
        }
    }
    if let Some(tone) = fx.play_tone {
        tones::play(tone, cfg_snapshot.ptt.tones_enabled, cfg_snapshot.ptt.tones_volume);
    }
    if let Some(led) = fx.set_led {
        if cfg_snapshot.ptt.led_enabled {
            if let Err(e) = wave_xlr::set_led(led) {
                eprintln!("ptt LED set failed: {}", e); // non-fatal
            }
        }
    }
    if let Some(mode) = fx.notify_mode {
        notify::notify_mode(mode);
    }
    if let Some(mode) = fx.persist_mode {
        let st = app.state::<AppState>();
        let mut cfg = st.config.lock().unwrap();
        cfg.ptt.mode = mode;
        if let Err(e) = crate::save_config_to_disk(&cfg) {
            eprintln!("ptt mode persist failed: {}", e);
        }
    }
    emit_state(app, runtime);
}

pub fn handle_toggle(app: &AppHandle, runtime: &Arc<PttRuntime>) {
    let mut s = runtime.state.lock().unwrap();
    let fx = state::toggle_mode(&mut s);
    drop(s);
    apply_effects(app, runtime, &fx);
}

pub fn handle_press(app: &AppHandle, runtime: &Arc<PttRuntime>) {
    let mut s = runtime.state.lock().unwrap();
    let fx = state::hold_press(&mut s);
    drop(s);
    apply_effects(app, runtime, &fx);
}

pub fn handle_release(app: &AppHandle, runtime: &Arc<PttRuntime>) {
    let mut s = runtime.state.lock().unwrap();
    let fx = state::hold_release(&mut s);
    drop(s);
    apply_effects(app, runtime, &fx);
}
