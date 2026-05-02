use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PluginConfig {
    #[serde(default = "default_led_enabled")]
    pub led_enabled: bool,
}

fn default_led_enabled() -> bool { true }

impl Default for PluginConfig {
    fn default() -> Self { Self { led_enabled: true } }
}

impl PluginConfig {
    pub fn from_value(v: &serde_json::Value) -> Self {
        if v.is_null() { return Self::default(); }
        serde_json::from_value(v.clone()).unwrap_or_default()
    }

    pub fn to_value(&self) -> serde_json::Value {
        serde_json::to_value(self).unwrap_or(serde_json::Value::Null)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn default_has_led_enabled_true() {
        assert!(PluginConfig::default().led_enabled);
    }

    #[test]
    fn from_null_value_uses_default() {
        let cfg = PluginConfig::from_value(&serde_json::Value::Null);
        assert!(cfg.led_enabled);
    }

    #[test]
    fn from_object_reads_led_enabled() {
        let v = json!({ "led_enabled": false });
        assert!(!PluginConfig::from_value(&v).led_enabled);
    }

    #[test]
    fn missing_led_enabled_defaults_true() {
        let v = json!({});
        assert!(PluginConfig::from_value(&v).led_enabled);
    }

    #[test]
    fn unknown_fields_round_trip_safe() {
        let v = json!({ "led_enabled": false, "future_color": "blue" });
        let cfg = PluginConfig::from_value(&v);
        assert!(!cfg.led_enabled);
        let back = cfg.to_value();
        assert_eq!(back, json!({ "led_enabled": false }));
    }
}
