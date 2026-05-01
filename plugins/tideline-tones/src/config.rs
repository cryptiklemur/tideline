use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TonesConfig {
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    #[serde(default = "default_volume")]
    pub volume: u32,
}

fn default_enabled() -> bool { true }
fn default_volume() -> u32 { 100 }

impl Default for TonesConfig {
    fn default() -> Self { Self { enabled: true, volume: 100 } }
}

impl TonesConfig {
    pub fn clamped_volume(&self) -> u32 { self.volume.min(100) }
    pub fn volume_scalar(&self) -> f32 { (self.clamped_volume() as f32 / 100.0).clamp(0.0, 1.0) }
    pub fn from_json(v: &serde_json::Value) -> Self {
        serde_json::from_value(v.clone()).unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test] fn default_is_enabled_full_volume() {
        let c = TonesConfig::default();
        assert!(c.enabled);
        assert_eq!(c.volume, 100);
        assert_eq!(c.volume_scalar(), 1.0);
    }

    #[test] fn missing_fields_use_defaults() {
        let v: serde_json::Value = serde_json::from_str("{}").unwrap();
        let c = TonesConfig::from_json(&v);
        assert_eq!(c, TonesConfig::default());
    }

    #[test] fn partial_fields_use_defaults() {
        let v: serde_json::Value = serde_json::from_str(r#"{"volume":50}"#).unwrap();
        let c = TonesConfig::from_json(&v);
        assert!(c.enabled);
        assert_eq!(c.volume, 50);
    }

    #[test] fn malformed_falls_back_to_default() {
        let v: serde_json::Value = serde_json::from_str(r#"{"enabled":"oops"}"#).unwrap();
        let c = TonesConfig::from_json(&v);
        assert_eq!(c, TonesConfig::default());
    }

    #[test] fn volume_scalar_clamps_above_100() {
        let c = TonesConfig { enabled: true, volume: 250 };
        assert_eq!(c.volume_scalar(), 1.0);
    }
}
