use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub use crate::host::Format as PluginFormat;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Effect {
    pub id: Uuid,
    pub format: PluginFormat,
    pub uri: String,
    pub display_name: String,
    pub bypassed: bool,
    pub state_b64: Option<String>,
}

/// Schema written into `channel.plugin_data["tideline-effects"]`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ChannelEffectsData {
    #[serde(default)]
    pub effects: Vec<Effect>,
    #[serde(default)]
    pub chain_bypassed: bool,
}

impl Effect {
    /// Convenience constructor for an LV2 effect with a fresh id, default name from URI.
    pub fn new_lv2(uri: impl Into<String>) -> Self {
        let uri = uri.into();
        Self {
            id: Uuid::new_v4(),
            format: PluginFormat::Lv2,
            uri: uri.clone(),
            display_name: uri,
            bypassed: false,
            state_b64: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn effect_round_trips_through_json() {
        let e = Effect {
            id: Uuid::nil(),
            format: PluginFormat::Lv2,
            uri: "http://lsp-plug.in/plugins/lv2/gate_mono".into(),
            display_name: "LSP Gate Mono".into(),
            bypassed: false,
            state_b64: Some("AAAA".into()),
        };
        let s = serde_json::to_string(&e).unwrap();
        let back: Effect = serde_json::from_str(&s).unwrap();
        assert_eq!(e, back);
    }

    #[test]
    fn channel_effects_data_defaults_to_empty() {
        let d: ChannelEffectsData = serde_json::from_str("{}").unwrap();
        assert!(d.effects.is_empty());
        assert!(!d.chain_bypassed);
    }

    #[test]
    fn plugin_format_serializes_lowercase() {
        assert_eq!(serde_json::to_string(&PluginFormat::Vst3).unwrap(), "\"vst3\"");
    }
}
