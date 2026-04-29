use crate::Mode;

/// Pure state container for PTT. Side effects (mute, LED, tones, notify, persist)
/// are returned as `Effects` so the caller can dispatch them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PttState {
    pub mode: Mode,
    pub hold_active: bool,
}

impl PttState {
    pub fn new(mode: Mode) -> Self { Self { mode, hold_active: false } }
    pub fn transmitting(&self) -> bool {
        match self.mode { Mode::Open => true, Mode::Ptt => self.hold_active }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Effects {
    pub set_muted: Option<bool>,    // Some(true)=mute, Some(false)=unmute, None=no change
    pub play_tone: Option<Tone>,    // None unless a hold transition happened
    pub set_led: Option<LedColor>,
    pub notify_mode: Option<Mode>,  // only set on mode toggle
    pub persist_mode: Option<Mode>, // only set on mode toggle
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)] pub enum Tone { Up, Down }
#[derive(Debug, Clone, Copy, PartialEq, Eq)] pub enum LedColor { Blue, Red }

impl Effects {
    fn empty() -> Self {
        Self { set_muted: None, play_tone: None, set_led: None, notify_mode: None, persist_mode: None }
    }
}

pub fn toggle_mode(s: &mut PttState) -> Effects {
    let mut fx = Effects::empty();
    s.mode = match s.mode { Mode::Open => Mode::Ptt, Mode::Ptt => Mode::Open };
    s.hold_active = false; // entering or leaving PTT resets hold
    fx.notify_mode = Some(s.mode);
    fx.persist_mode = Some(s.mode);
    match s.mode {
        Mode::Open => { fx.set_muted = Some(false); fx.set_led = Some(LedColor::Blue); }
        Mode::Ptt  => { fx.set_muted = Some(true);  fx.set_led = Some(LedColor::Red);  }
    }
    fx
}

pub fn hold_press(s: &mut PttState) -> Effects {
    let mut fx = Effects::empty();
    if s.mode != Mode::Ptt || s.hold_active { return fx; }
    s.hold_active = true;
    fx.set_muted = Some(false);
    fx.play_tone = Some(Tone::Up);
    fx.set_led   = Some(LedColor::Blue);
    fx
}

pub fn hold_release(s: &mut PttState) -> Effects {
    let mut fx = Effects::empty();
    if s.mode != Mode::Ptt || !s.hold_active { return fx; }
    s.hold_active = false;
    fx.set_muted = Some(true);
    fx.play_tone = Some(Tone::Down);
    fx.set_led   = Some(LedColor::Red);
    fx
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test] fn open_to_ptt_mutes_and_red() {
        let mut s = PttState::new(Mode::Open);
        let fx = toggle_mode(&mut s);
        assert_eq!(s.mode, Mode::Ptt);
        assert_eq!(fx.set_muted, Some(true));
        assert_eq!(fx.set_led,   Some(LedColor::Red));
        assert_eq!(fx.notify_mode, Some(Mode::Ptt));
        assert_eq!(fx.persist_mode, Some(Mode::Ptt));
    }

    #[test] fn ptt_to_open_unmutes_and_blue() {
        let mut s = PttState::new(Mode::Ptt);
        let fx = toggle_mode(&mut s);
        assert_eq!(s.mode, Mode::Open);
        assert_eq!(fx.set_muted, Some(false));
        assert_eq!(fx.set_led,   Some(LedColor::Blue));
    }

    #[test] fn hold_noop_in_open() {
        let mut s = PttState::new(Mode::Open);
        let fx = hold_press(&mut s);
        assert_eq!(s.hold_active, false);
        assert_eq!(fx, Effects::empty());
        let fx = hold_release(&mut s);
        assert_eq!(fx, Effects::empty());
    }

    #[test] fn hold_press_in_ptt_unmutes_and_blue() {
        let mut s = PttState::new(Mode::Ptt);
        let fx = hold_press(&mut s);
        assert!(s.hold_active);
        assert_eq!(fx.set_muted, Some(false));
        assert_eq!(fx.set_led,   Some(LedColor::Blue));
        assert_eq!(fx.play_tone, Some(Tone::Up));
        assert_eq!(fx.notify_mode, None); // no notification on hold
    }

    #[test] fn hold_release_in_ptt_mutes_and_red() {
        let mut s = PttState { mode: Mode::Ptt, hold_active: true };
        let fx = hold_release(&mut s);
        assert!(!s.hold_active);
        assert_eq!(fx.set_muted, Some(true));
        assert_eq!(fx.set_led,   Some(LedColor::Red));
        assert_eq!(fx.play_tone, Some(Tone::Down));
    }

    #[test] fn double_press_idempotent() {
        let mut s = PttState::new(Mode::Ptt);
        hold_press(&mut s);
        let fx2 = hold_press(&mut s);
        assert!(s.hold_active);
        assert_eq!(fx2, Effects::empty());
    }

    #[test] fn double_release_idempotent() {
        let mut s = PttState::new(Mode::Ptt);
        let fx = hold_release(&mut s);
        assert_eq!(fx, Effects::empty());
    }

    #[test] fn toggle_to_ptt_resets_hold() {
        let mut s = PttState { mode: Mode::Open, hold_active: false };
        toggle_mode(&mut s); // -> Ptt, hold=false (and mute)
        s.hold_active = true; // pretend hold was pressed
        toggle_mode(&mut s); // -> Open, hold should reset to false
        assert!(!s.hold_active);
    }

    #[test] fn transmitting_open_always_true() {
        assert!(PttState::new(Mode::Open).transmitting());
    }
    #[test] fn transmitting_ptt_follows_hold() {
        assert!(!PttState::new(Mode::Ptt).transmitting());
        let s = PttState { mode: Mode::Ptt, hold_active: true };
        assert!(s.transmitting());
    }
}
