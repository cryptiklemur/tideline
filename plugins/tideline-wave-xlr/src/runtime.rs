use crate::config::PluginConfig;
use crate::device::{LedColor, LedWriter};
use std::sync::Mutex;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct LastWrite {
    color: LedColor,
}

pub struct Runtime {
    config: Mutex<PluginConfig>,
    last_write: Mutex<Option<LastWrite>>,
    writer: Box<dyn LedWriter>,
    present: bool,
}

impl Runtime {
    pub fn new(config: PluginConfig, writer: Box<dyn LedWriter>, present: bool) -> Self {
        Self {
            config: Mutex::new(config),
            last_write: Mutex::new(None),
            writer,
            present,
        }
    }

    /// Handles `tideline-ptt:transmit_changed { transmitting: bool }`.
    /// - device not present: no-op.
    /// - led_enabled false: no-op.
    /// - else: map transmitting -> color, write iff different from last.
    pub fn on_transmit_changed(&self, transmitting: bool) {
        if !self.present { return; }
        let cfg = self.config.lock().unwrap().clone();
        if !cfg.led_enabled { return; }
        let color = if transmitting { LedColor::Blue } else { LedColor::Red };
        let mut last = self.last_write.lock().unwrap();
        if last.map(|l| l.color) == Some(color) { return; }
        match self.writer.set_led(color) {
            Ok(()) => { *last = Some(LastWrite { color }); }
            Err(e) => { eprintln!("wave-xlr: led set failed: {e}"); }
        }
    }

    /// Handles a settings section UI event. Returns the updated config when
    /// it actually changed, so the caller can persist via
    /// `host/config.namespace.set`.
    pub fn on_settings_event(&self, event_id: &str, value: serde_json::Value) -> Option<PluginConfig> {
        if event_id != "led_enabled" { return None; }
        let new = value.as_bool()?;
        let mut cfg = self.config.lock().unwrap();
        if cfg.led_enabled == new { return None; }
        cfg.led_enabled = new;
        Some(cfg.clone())
    }

    pub fn config_snapshot(&self) -> PluginConfig {
        self.config.lock().unwrap().clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex as StdMutex};

    #[derive(Default)]
    struct RecordingWriter {
        calls: StdMutex<Vec<LedColor>>,
    }

    impl LedWriter for RecordingWriter {
        fn set_led(&self, color: LedColor) -> Result<(), String> {
            self.calls.lock().unwrap().push(color);
            Ok(())
        }
    }

    struct SharedWriter(Arc<RecordingWriter>);
    impl LedWriter for SharedWriter {
        fn set_led(&self, color: LedColor) -> Result<(), String> {
            self.0.set_led(color)
        }
    }

    fn make(present: bool, led_enabled: bool) -> (Runtime, Arc<RecordingWriter>) {
        let w = Arc::new(RecordingWriter::default());
        let r = Runtime::new(
            PluginConfig { led_enabled },
            Box::new(SharedWriter(w.clone())),
            present,
        );
        (r, w)
    }

    #[test]
    fn transmit_true_writes_blue() {
        let (r, w) = make(true, true);
        r.on_transmit_changed(true);
        assert_eq!(w.calls.lock().unwrap().clone(), vec![LedColor::Blue]);
    }

    #[test]
    fn transmit_false_writes_red() {
        let (r, w) = make(true, true);
        r.on_transmit_changed(false);
        assert_eq!(w.calls.lock().unwrap().clone(), vec![LedColor::Red]);
    }

    #[test]
    fn led_disabled_skips_write() {
        let (r, w) = make(true, false);
        r.on_transmit_changed(true);
        assert!(w.calls.lock().unwrap().is_empty());
    }

    #[test]
    fn device_not_present_skips_write() {
        let (r, w) = make(false, true);
        r.on_transmit_changed(true);
        assert!(w.calls.lock().unwrap().is_empty());
    }

    #[test]
    fn duplicate_transmit_value_dedups() {
        let (r, w) = make(true, true);
        r.on_transmit_changed(true);
        r.on_transmit_changed(true);
        r.on_transmit_changed(true);
        assert_eq!(w.calls.lock().unwrap().len(), 1);
    }

    #[test]
    fn alternating_writes_both() {
        let (r, w) = make(true, true);
        r.on_transmit_changed(true);
        r.on_transmit_changed(false);
        r.on_transmit_changed(true);
        assert_eq!(
            w.calls.lock().unwrap().clone(),
            vec![LedColor::Blue, LedColor::Red, LedColor::Blue]
        );
    }

    #[test]
    fn settings_event_changes_config() {
        let (r, _w) = make(true, true);
        let updated = r.on_settings_event("led_enabled", serde_json::json!(false));
        assert!(!updated.unwrap().led_enabled);
        assert!(!r.config_snapshot().led_enabled);
    }

    #[test]
    fn settings_event_no_change_returns_none() {
        let (r, _w) = make(true, true);
        let updated = r.on_settings_event("led_enabled", serde_json::json!(true));
        assert!(updated.is_none());
    }

    #[test]
    fn settings_event_unknown_id_returns_none() {
        let (r, _w) = make(true, true);
        let updated = r.on_settings_event("color", serde_json::json!("blue"));
        assert!(updated.is_none());
    }

    #[test]
    fn settings_event_non_bool_returns_none() {
        let (r, _w) = make(true, true);
        let updated = r.on_settings_event("led_enabled", serde_json::json!("yes"));
        assert!(updated.is_none());
    }
}
