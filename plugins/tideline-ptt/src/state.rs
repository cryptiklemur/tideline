use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    #[default]
    Open,
    Ptt,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PerSourceState {
    pub mode: Mode,
    pub hold_active: bool,
}

impl Default for PerSourceState {
    fn default() -> Self {
        Self {
            mode: Mode::Open,
            hold_active: false,
        }
    }
}

impl PerSourceState {
    pub fn transmitting(&self) -> bool {
        match self.mode {
            Mode::Open => true,
            Mode::Ptt => self.hold_active,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Effects {
    pub source_name: String,
    pub set_muted: Option<bool>,
    pub mode_changed: Option<Mode>,
    pub transmit_changed: Option<bool>,
    pub notify_mode: Option<Mode>,
    pub persist_mode: Option<Mode>,
}

pub fn toggle_mode(state: &mut PerSourceState, source: &str) -> Effects {
    let old_transmitting = state.transmitting();
    let new_mode = match state.mode {
        Mode::Open => Mode::Ptt,
        Mode::Ptt => Mode::Open,
    };
    state.mode = new_mode;
    state.hold_active = false;
    let new_transmitting = state.transmitting();
    Effects {
        source_name: source.to_string(),
        set_muted: Some(!new_transmitting),
        mode_changed: Some(new_mode),
        transmit_changed: if old_transmitting != new_transmitting {
            Some(new_transmitting)
        } else {
            None
        },
        notify_mode: Some(new_mode),
        persist_mode: Some(new_mode),
    }
}

pub fn hold_press(state: &mut PerSourceState, source: &str) -> Effects {
    if state.mode != Mode::Ptt || state.hold_active {
        return Effects {
            source_name: source.to_string(),
            ..Default::default()
        };
    }
    state.hold_active = true;
    Effects {
        source_name: source.to_string(),
        set_muted: Some(false),
        transmit_changed: Some(true),
        ..Default::default()
    }
}

pub fn hold_release(state: &mut PerSourceState, source: &str) -> Effects {
    if state.mode != Mode::Ptt || !state.hold_active {
        return Effects {
            source_name: source.to_string(),
            ..Default::default()
        };
    }
    state.hold_active = false;
    Effects {
        source_name: source.to_string(),
        set_muted: Some(true),
        transmit_changed: Some(false),
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SRC: &str = "alsa_input.test";

    #[test]
    fn open_to_ptt_mutes() {
        let mut s = PerSourceState {
            mode: Mode::Open,
            hold_active: false,
        };
        let e = toggle_mode(&mut s, SRC);
        assert_eq!(s.mode, Mode::Ptt);
        assert!(!s.hold_active);
        assert_eq!(e.source_name, SRC);
        assert_eq!(e.set_muted, Some(true));
        assert_eq!(e.mode_changed, Some(Mode::Ptt));
        assert_eq!(e.transmit_changed, Some(false));
    }

    #[test]
    fn ptt_to_open_unmutes() {
        let mut s = PerSourceState {
            mode: Mode::Ptt,
            hold_active: false,
        };
        let e = toggle_mode(&mut s, SRC);
        assert_eq!(s.mode, Mode::Open);
        assert_eq!(e.source_name, SRC);
        assert_eq!(e.set_muted, Some(false));
        assert_eq!(e.mode_changed, Some(Mode::Open));
        assert_eq!(e.transmit_changed, Some(true));
    }

    #[test]
    fn hold_noop_in_open() {
        let mut s = PerSourceState {
            mode: Mode::Open,
            hold_active: false,
        };
        let e_press = hold_press(&mut s, SRC);
        let e_release = hold_release(&mut s, SRC);
        assert_eq!(e_press.set_muted, None);
        assert_eq!(e_press.transmit_changed, None);
        assert_eq!(e_release.set_muted, None);
        assert_eq!(e_release.transmit_changed, None);
        assert!(!s.hold_active);
    }

    #[test]
    fn hold_press_in_ptt_unmutes() {
        let mut s = PerSourceState {
            mode: Mode::Ptt,
            hold_active: false,
        };
        let e = hold_press(&mut s, SRC);
        assert!(s.hold_active);
        assert_eq!(e.source_name, SRC);
        assert_eq!(e.set_muted, Some(false));
        assert_eq!(e.transmit_changed, Some(true));
    }

    #[test]
    fn hold_release_in_ptt_mutes() {
        let mut s = PerSourceState {
            mode: Mode::Ptt,
            hold_active: true,
        };
        let e = hold_release(&mut s, SRC);
        assert!(!s.hold_active);
        assert_eq!(e.source_name, SRC);
        assert_eq!(e.set_muted, Some(true));
        assert_eq!(e.transmit_changed, Some(false));
    }

    #[test]
    fn double_press_idempotent() {
        let mut s = PerSourceState {
            mode: Mode::Ptt,
            hold_active: false,
        };
        let _ = hold_press(&mut s, SRC);
        let e = hold_press(&mut s, SRC);
        assert_eq!(e.set_muted, None);
        assert_eq!(e.transmit_changed, None);
        assert!(s.hold_active);
    }

    #[test]
    fn double_release_idempotent() {
        let mut s = PerSourceState {
            mode: Mode::Ptt,
            hold_active: false,
        };
        let e = hold_release(&mut s, SRC);
        assert_eq!(e.set_muted, None);
        assert_eq!(e.transmit_changed, None);
        assert!(!s.hold_active);
    }

    #[test]
    fn toggle_to_ptt_resets_hold() {
        let mut s = PerSourceState {
            mode: Mode::Open,
            hold_active: true,
        };
        let e = toggle_mode(&mut s, SRC);
        assert_eq!(s.mode, Mode::Ptt);
        assert!(!s.hold_active);
        assert_eq!(e.set_muted, Some(true));
    }

    #[test]
    fn transmitting_open_always_true() {
        let s = PerSourceState {
            mode: Mode::Open,
            hold_active: false,
        };
        assert!(s.transmitting());
    }

    #[test]
    fn transmitting_ptt_follows_hold() {
        let s_off = PerSourceState {
            mode: Mode::Ptt,
            hold_active: false,
        };
        let s_on = PerSourceState {
            mode: Mode::Ptt,
            hold_active: true,
        };
        assert!(!s_off.transmitting());
        assert!(s_on.transmitting());
    }
}
