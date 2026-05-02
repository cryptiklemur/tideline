pub mod binding;
pub mod evdev_listener;
pub mod mute;
pub mod portal_listener;
pub mod state;
pub mod wave_xlr;

use crate::ptt::state::Tone;
use crate::{AppConfig, AppState, Mode};
use serde::Serialize;
use state::Effects;
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, Manager};

fn tone_str(t: Option<Tone>) -> serde_json::Value {
    match t {
        Some(Tone::Up) => serde_json::Value::String("up".into()),
        Some(Tone::Down) => serde_json::Value::String("down".into()),
        None => serde_json::Value::Null,
    }
}

fn mode_str(m: Mode) -> &'static str {
    match m {
        Mode::Open => "open",
        Mode::Ptt => "ptt",
    }
}

/// Pure helper - takes a publish closure so it is testable without a real bus.
pub(crate) fn publish_ptt_events<F: Fn(&str, serde_json::Value)>(
    publish: &F,
    state: &state::PttState,
    fx: &state::Effects,
) {
    publish(
        "tideline-ptt:state_changed",
        serde_json::json!({
            "mode":         mode_str(state.mode),
            "hold_active":  state.hold_active,
            "transmitting": state.transmitting(),
            "play_tone":    tone_str(fx.play_tone),
        }),
    );
    if let Some(mode) = fx.notify_mode {
        publish(
            "tideline-ptt:mode_changed",
            serde_json::json!({ "mode": mode_str(mode) }),
        );
    }
}

/// Which capture path is providing PTT shortcuts. Determines what UI surface
/// the Settings panel renders (portal info vs. raw evdev binding capture).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum CaptureMethod {
    /// No listener active yet (startup, or both paths failed).
    #[default]
    None,
    /// XDG GlobalShortcuts portal (Wayland-native).
    Portal,
    /// `/dev/input/event*` raw capture (legacy / fallback).
    Evdev,
}

/// Runtime owned by Tauri's state, shared by listeners and Tauri commands.
pub struct PttRuntime {
    pub state: Mutex<state::PttState>,
    pub last_error: Mutex<Option<String>>,
    pub capture_method: Mutex<CaptureMethod>,
}

impl PttRuntime {
    pub fn from_config(cfg: &AppConfig) -> Arc<Self> {
        Arc::new(Self {
            state: Mutex::new(state::PttState::new(cfg.ptt.mode)),
            last_error: Mutex::new(None),
            capture_method: Mutex::new(CaptureMethod::None),
        })
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct PttStateEvent {
    pub mode: Mode,
    pub hold_active: bool,
    pub transmitting: bool,
    pub error: Option<String>,
    pub capture_method: CaptureMethod,
}

fn emit_state(app: &AppHandle, runtime: &PttRuntime) {
    let s = *runtime.state.lock().unwrap();
    let err = runtime.last_error.lock().unwrap().clone();
    let method = *runtime.capture_method.lock().unwrap();
    let _ = app.emit("ptt:state", PttStateEvent {
        mode: s.mode,
        hold_active: s.hold_active,
        transmitting: s.transmitting(),
        error: err,
        capture_method: method,
    });
}

/// Set which capture path is active and emit an updated state event.
pub fn set_capture_method(app: &AppHandle, runtime: &PttRuntime, method: CaptureMethod) {
    *runtime.capture_method.lock().unwrap() = method;
    emit_state(app, runtime);
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
    if let Some(led) = fx.set_led {
        if cfg_snapshot.ptt.led_enabled {
            if let Err(e) = wave_xlr::set_led(led) {
                eprintln!("ptt LED set failed: {}", e); // non-fatal
            }
        }
    }
    if let Some(mode) = fx.persist_mode {
        let st = app.state::<AppState>();
        let mut cfg = st.config.lock().unwrap();
        cfg.ptt.mode = mode;
        if let Err(e) = crate::save_config_to_disk(&cfg) {
            eprintln!("ptt mode persist failed: {}", e);
        }
    }
    {
        let registry = app
            .state::<Arc<tideline_host::PluginRegistry>>()
            .inner()
            .clone();
        let bus = registry.bus.clone();
        let s_now = *runtime.state.lock().unwrap();
        let fx_clone = fx.clone();
        tauri::async_runtime::spawn(async move {
            let publish = |topic: &str, payload: serde_json::Value| {
                let bus = bus.clone();
                let topic = topic.to_string();
                tauri::async_runtime::spawn(async move {
                    bus.publish_native(&topic, payload).await;
                });
            };
            publish_ptt_events(&publish, &s_now, &fx_clone);
        });
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

#[cfg(test)]
mod publish_tests {
    use super::*;
    use crate::ptt::state::{self, Effects, LedColor, Tone};
    use crate::Mode;

    #[derive(Default, Clone)]
    struct CapturedBus {
        events: std::sync::Arc<std::sync::Mutex<Vec<(String, serde_json::Value)>>>,
    }
    impl CapturedBus {
        fn publish(&self, topic: &str, payload: serde_json::Value) {
            self.events.lock().unwrap().push((topic.to_string(), payload));
        }
        fn drain(&self) -> Vec<(String, serde_json::Value)> {
            std::mem::take(&mut *self.events.lock().unwrap())
        }
    }

    fn make_state(mode: Mode, hold: bool) -> state::PttState {
        state::PttState { mode, hold_active: hold }
    }
    fn bus_pub<'a>(b: &'a CapturedBus) -> impl Fn(&str, serde_json::Value) + 'a {
        move |t, p| b.publish(t, p)
    }

    #[test]
    fn hold_press_publishes_state_changed_with_play_tone_up() {
        let bus = CapturedBus::default();
        let s = make_state(Mode::Ptt, true);
        let fx = Effects {
            set_muted: Some(false),
            play_tone: Some(Tone::Up),
            set_led: Some(LedColor::Blue),
            notify_mode: None,
            persist_mode: None,
        };
        publish_ptt_events(&bus_pub(&bus), &s, &fx);
        let events = bus.drain();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].0, "tideline-ptt:state_changed");
        assert_eq!(events[0].1["mode"], "ptt");
        assert_eq!(events[0].1["hold_active"], true);
        assert_eq!(events[0].1["transmitting"], true);
        assert_eq!(events[0].1["play_tone"], "up");
    }

    #[test]
    fn hold_release_publishes_play_tone_down() {
        let bus = CapturedBus::default();
        let s = make_state(Mode::Ptt, false);
        let fx = Effects {
            set_muted: Some(true),
            play_tone: Some(Tone::Down),
            set_led: Some(LedColor::Red),
            notify_mode: None,
            persist_mode: None,
        };
        publish_ptt_events(&bus_pub(&bus), &s, &fx);
        let events = bus.drain();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].1["play_tone"], "down");
    }

    #[test]
    fn toggle_mode_publishes_both_state_and_mode_changed() {
        let bus = CapturedBus::default();
        let s = make_state(Mode::Ptt, false);
        let fx = Effects {
            set_muted: Some(true),
            play_tone: None,
            set_led: Some(LedColor::Red),
            notify_mode: Some(Mode::Ptt),
            persist_mode: Some(Mode::Ptt),
        };
        publish_ptt_events(&bus_pub(&bus), &s, &fx);
        let events = bus.drain();
        assert_eq!(events.len(), 2);
        let topics: Vec<&str> = events.iter().map(|(t, _)| t.as_str()).collect();
        assert!(topics.contains(&"tideline-ptt:state_changed"));
        assert!(topics.contains(&"tideline-ptt:mode_changed"));
        let mode_evt = events.iter().find(|(t, _)| t == "tideline-ptt:mode_changed").unwrap();
        assert_eq!(mode_evt.1["mode"], "ptt");
        let state_evt = events.iter().find(|(t, _)| t == "tideline-ptt:state_changed").unwrap();
        assert_eq!(state_evt.1["play_tone"], serde_json::Value::Null);
    }

    #[test]
    fn idle_apply_publishes_state_changed_with_null_tone() {
        let bus = CapturedBus::default();
        let s = make_state(Mode::Open, false);
        let fx = Effects {
            set_muted: None,
            play_tone: None,
            set_led: None,
            notify_mode: None,
            persist_mode: None,
        };
        publish_ptt_events(&bus_pub(&bus), &s, &fx);
        let events = bus.drain();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].0, "tideline-ptt:state_changed");
        assert_eq!(events[0].1["play_tone"], serde_json::Value::Null);
    }
}
