use crate::binding::Binding;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ChannelKind {
    #[default]
    Output,
    Input,
    PhysicalInput,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Mix {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub sinks: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelCfg {
    pub name: String,
    #[serde(default)]
    pub kind: ChannelKind,
    #[serde(default)]
    pub hp_node: String,
    #[serde(default)]
    pub sp_node: String,
    #[serde(default)]
    pub programs: Vec<String>,
    #[serde(default)]
    pub sources: Vec<String>,
    #[serde(default)]
    pub physical_source: String,
    #[serde(default)]
    pub icon: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum KeybindAction {
    ToggleOutputMute { sink: String },
    ToggleChannelMute { channel: String },
    ToggleMixEnabled { mix_id: String },
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Mode { #[default] Open, Ptt }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PttConfig {
    #[serde(default)]
    pub mode: Mode,
    #[serde(default)]
    pub mode_toggle_binding: Option<Binding>,
    #[serde(default)]
    pub hold_binding: Option<Binding>,
    #[serde(default)]
    pub input_device: String,
    #[serde(default = "default_tones_enabled")]
    pub tones_enabled: bool,
    #[serde(default = "default_tones_volume")]
    pub tones_volume: u32,
    #[serde(default = "default_led_enabled")]
    pub led_enabled: bool,
}

fn default_tones_enabled() -> bool { true }
fn default_tones_volume() -> u32 { 100 }
fn default_led_enabled() -> bool { true }

impl Default for PttConfig {
    fn default() -> Self {
        Self {
            mode: Mode::Open,
            mode_toggle_binding: None,
            hold_binding: None,
            input_device: String::new(),
            tones_enabled: true,
            tones_volume: 100,
            led_enabled: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    #[serde(default)]
    pub mixes: Vec<Mix>,
    pub channels: Vec<ChannelCfg>,
    #[serde(default)]
    pub keybinds: HashMap<String, KeybindAction>,
    #[serde(default)]
    pub ptt: PttConfig,
}
