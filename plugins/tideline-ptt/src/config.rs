use crate::binding::Binding;
use crate::state::Mode;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tideline_sdk::transport::SdkTransportError;
use tideline_sdk::HostClient;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct PluginConfig {
    #[serde(default)]
    pub mode_toggle_binding: Option<Binding>,
    #[serde(default)]
    pub hold_binding: Option<Binding>,
    #[serde(default)]
    pub enabled_sources: Vec<String>,
    #[serde(default)]
    pub mode_by_source: HashMap<String, Mode>,
}

impl PluginConfig {
    pub fn from_value(v: &serde_json::Value) -> Self {
        if v.is_null() {
            return Self::default();
        }
        let mut cfg: PluginConfig = serde_json::from_value(v.clone()).unwrap_or_default();
        if cfg.enabled_sources.is_empty() {
            if let Some(legacy_input) = v.get("input_device").and_then(|x| x.as_str()) {
                if !legacy_input.is_empty() {
                    cfg.enabled_sources.push(legacy_input.to_string());
                    if let Some(legacy_mode_str) = v.get("mode").and_then(|x| x.as_str()) {
                        if let Ok(m) = serde_json::from_value::<Mode>(serde_json::Value::String(
                            legacy_mode_str.to_string(),
                        )) {
                            cfg.mode_by_source.insert(legacy_input.to_string(), m);
                        }
                    }
                }
            }
        }
        cfg
    }

    pub fn to_value(&self) -> serde_json::Value {
        serde_json::to_value(self).unwrap_or(serde_json::Value::Null)
    }

    pub fn mode_for(&self, source: &str) -> Mode {
        self.mode_by_source
            .get(source)
            .copied()
            .unwrap_or(Mode::Open)
    }
}

#[allow(dead_code)]
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
    fn default_has_no_bindings_or_sources() {
        let cfg = PluginConfig::default();
        assert!(cfg.mode_toggle_binding.is_none());
        assert!(cfg.hold_binding.is_none());
        assert!(cfg.enabled_sources.is_empty());
        assert!(cfg.mode_by_source.is_empty());
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
    fn round_trips_with_bindings_and_sources() {
        let mut mode_by_source = HashMap::new();
        mode_by_source.insert("alsa_input.usb-Elgato.analog-stereo".to_string(), Mode::Ptt);
        let cfg = PluginConfig {
            mode_toggle_binding: Some(Binding::Keyboard {
                mods: vec![Modifier::Ctrl],
                key: "F12".to_string(),
            }),
            hold_binding: Some(Binding::Mouse {
                mods: vec![],
                button: "Mouse5".to_string(),
            }),
            enabled_sources: vec!["alsa_input.usb-Elgato.analog-stereo".to_string()],
            mode_by_source,
        };
        let v = cfg.to_value();
        let back = PluginConfig::from_value(&v);
        assert_eq!(back, cfg);
    }

    #[test]
    fn unknown_fields_are_tolerated() {
        let v = json!({
            "future_field": "ignored",
            "enabled_sources": ["src1"],
        });
        let cfg = PluginConfig::from_value(&v);
        assert_eq!(cfg.enabled_sources, vec!["src1".to_string()]);
    }

    #[test]
    fn missing_fields_use_defaults() {
        let v = json!({ "enabled_sources": ["src1"] });
        let cfg = PluginConfig::from_value(&v);
        assert!(cfg.mode_toggle_binding.is_none());
        assert!(cfg.hold_binding.is_none());
        assert_eq!(cfg.enabled_sources, vec!["src1".to_string()]);
        assert!(cfg.mode_by_source.is_empty());
    }

    #[test]
    fn legacy_bindings_by_source_field_is_ignored() {
        let v = json!({
            "enabled_sources": ["src1"],
            "bindings_by_source": {
                "src1": { "mode_toggle": { "Keyboard": { "mods": [], "key": "F1" } } }
            },
        });
        let cfg = PluginConfig::from_value(&v);
        assert_eq!(cfg.enabled_sources, vec!["src1".to_string()]);
    }

    #[test]
    fn mode_for_returns_open_when_unset() {
        let cfg = PluginConfig::default();
        assert_eq!(cfg.mode_for("unknown"), Mode::Open);
    }

    #[test]
    fn mode_for_returns_persisted_value() {
        let mut mode_by_source = HashMap::new();
        mode_by_source.insert("src1".to_string(), Mode::Ptt);
        let cfg = PluginConfig {
            mode_by_source,
            ..PluginConfig::default()
        };
        assert_eq!(cfg.mode_for("src1"), Mode::Ptt);
        assert_eq!(cfg.mode_for("src2"), Mode::Open);
    }
}
