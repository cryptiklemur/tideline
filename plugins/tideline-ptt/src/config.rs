use crate::binding::Binding;
use crate::state::Mode;
use serde::{Deserialize, Serialize};
use tideline_sdk::transport::SdkTransportError;
use tideline_sdk::HostClient;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PluginConfig {
    #[serde(default)]
    pub mode: Mode,
    #[serde(default)]
    pub mode_toggle_binding: Option<Binding>,
    #[serde(default)]
    pub hold_binding: Option<Binding>,
    #[serde(default)]
    pub input_device: Option<String>,
}

impl Default for PluginConfig {
    fn default() -> Self {
        Self {
            mode: Mode::Open,
            mode_toggle_binding: None,
            hold_binding: None,
            input_device: None,
        }
    }
}

impl PluginConfig {
    pub fn from_value(v: &serde_json::Value) -> Self {
        if v.is_null() {
            return Self::default();
        }
        serde_json::from_value(v.clone()).unwrap_or_default()
    }

    pub fn to_value(&self) -> serde_json::Value {
        serde_json::to_value(self).unwrap_or(serde_json::Value::Null)
    }
}

pub async fn load(client: &HostClient, namespace: &str) -> Result<PluginConfig, SdkTransportError> {
    let raw = client.config_namespace_get(namespace).await?;
    Ok(PluginConfig::from_value(&raw))
}

pub async fn save(
    client: &HostClient,
    namespace: &str,
    cfg: &PluginConfig,
) -> Result<(), SdkTransportError> {
    client.config_namespace_set(namespace, cfg.to_value()).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::binding::{Binding, Modifier};
    use serde_json::json;

    #[test]
    fn default_is_open_mode_with_no_bindings() {
        let cfg = PluginConfig::default();
        assert_eq!(cfg.mode, Mode::Open);
        assert!(cfg.mode_toggle_binding.is_none());
        assert!(cfg.hold_binding.is_none());
        assert!(cfg.input_device.is_none());
    }

    #[test]
    fn from_null_value_uses_default() {
        let cfg = PluginConfig::from_value(&serde_json::Value::Null);
        assert_eq!(cfg, PluginConfig::default());
    }

    #[test]
    fn from_empty_object_uses_default() {
        let cfg = PluginConfig::from_value(&json!({}));
        assert_eq!(cfg, PluginConfig::default());
    }

    #[test]
    fn round_trips_with_mode_and_bindings() {
        let cfg = PluginConfig {
            mode: Mode::Ptt,
            mode_toggle_binding: Some(Binding::Keyboard {
                mods: vec![Modifier::Ctrl],
                key: "F12".to_string(),
            }),
            hold_binding: Some(Binding::Mouse {
                mods: vec![],
                button: "Mouse5".to_string(),
            }),
            input_device: Some("/dev/input/event7".to_string()),
        };
        let v = cfg.to_value();
        let back = PluginConfig::from_value(&v);
        assert_eq!(back, cfg);
    }

    #[test]
    fn unknown_fields_are_tolerated() {
        let v = json!({
            "mode": "ptt",
            "future_field": "ignored",
        });
        let cfg = PluginConfig::from_value(&v);
        assert_eq!(cfg.mode, Mode::Ptt);
    }

    #[test]
    fn missing_fields_use_defaults() {
        let v = json!({ "mode": "ptt" });
        let cfg = PluginConfig::from_value(&v);
        assert_eq!(cfg.mode, Mode::Ptt);
        assert!(cfg.mode_toggle_binding.is_none());
        assert!(cfg.hold_binding.is_none());
        assert!(cfg.input_device.is_none());
    }
}
