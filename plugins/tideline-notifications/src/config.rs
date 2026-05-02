use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NotifConfig {
    #[serde(default = "default_enabled")]
    pub enabled: bool,
}

fn default_enabled() -> bool { true }

impl Default for NotifConfig {
    fn default() -> Self { Self { enabled: true } }
}

impl NotifConfig {
    pub fn from_json(v: &serde_json::Value) -> Self {
        serde_json::from_value(v.clone()).unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test] fn default_is_enabled() {
        assert!(NotifConfig::default().enabled);
    }

    #[test] fn missing_fields_use_defaults() {
        let v: serde_json::Value = serde_json::from_str("{}").unwrap();
        assert_eq!(NotifConfig::from_json(&v), NotifConfig::default());
    }

    #[test] fn explicit_disabled_round_trips() {
        let v: serde_json::Value = serde_json::from_str(r#"{"enabled":false}"#).unwrap();
        let c = NotifConfig::from_json(&v);
        assert!(!c.enabled);
    }

    #[test] fn malformed_falls_back_to_default() {
        let v: serde_json::Value = serde_json::from_str(r#"{"enabled":"nope"}"#).unwrap();
        assert_eq!(NotifConfig::from_json(&v), NotifConfig::default());
    }
}
