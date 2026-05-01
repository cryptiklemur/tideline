use crate::binding::Binding;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use uuid::Uuid;

fn fresh_uuid() -> Uuid { Uuid::new_v4() }

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ChannelKind {
    #[default]
    Output,
    Input,
    PhysicalInput,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Mix {
    #[serde(default = "fresh_uuid")]
    pub uuid: Uuid,
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub sinks: Vec<String>,
    #[serde(default)]
    pub plugin_data: HashMap<String, Value>,
}

impl Mix {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            uuid: Uuid::new_v4(),
            id: id.into(),
            name: name.into(),
            sinks: Vec::new(),
            plugin_data: HashMap::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelCfg {
    #[serde(default = "fresh_uuid")]
    pub uuid: Uuid,
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
    #[serde(default)]
    pub plugin_data: HashMap<String, Value>,
}

impl ChannelCfg {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            uuid: Uuid::new_v4(),
            name: name.into(),
            kind: ChannelKind::Output,
            hp_node: String::new(),
            sp_node: String::new(),
            programs: Vec::new(),
            sources: Vec::new(),
            physical_source: String::new(),
            icon: String::new(),
            plugin_data: HashMap::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum KeybindAction {
    ToggleOutputMute { sink: String },
    ToggleChannelMute { channel: String },
    ToggleMixEnabled { mix_id: String },
    Plugin { plugin_id: String, action_id: String },
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

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AppConfig {
    #[serde(default)]
    pub mixes: Vec<Mix>,
    #[serde(default)]
    pub channels: Vec<ChannelCfg>,
    #[serde(default)]
    pub keybinds: HashMap<String, KeybindAction>,
    #[serde(default)]
    pub ptt: PttConfig,
}

impl AppConfig {
    pub fn channel_by_uuid(&self, uuid: Uuid) -> Option<&ChannelCfg> {
        self.channels.iter().find(|c| c.uuid == uuid)
    }
    pub fn channel_by_uuid_mut(&mut self, uuid: Uuid) -> Option<&mut ChannelCfg> {
        self.channels.iter_mut().find(|c| c.uuid == uuid)
    }
    pub fn channel_by_name(&self, name: &str) -> Option<&ChannelCfg> {
        self.channels.iter().find(|c| c.name == name)
    }
    pub fn mix_by_id(&self, id: &str) -> Option<&Mix> {
        self.mixes.iter().find(|m| m.id == id)
    }
    pub fn mix_by_uuid_mut(&mut self, uuid: Uuid) -> Option<&mut Mix> {
        self.mixes.iter_mut().find(|m| m.uuid == uuid)
    }
    pub fn mix_uuid_for_id(&self, id: &str) -> Option<Uuid> {
        self.mixes.iter().find(|m| m.id == id).map(|m| m.uuid)
    }
    pub fn remove_channel_by_uuid(&mut self, uuid: Uuid) {
        self.channels.retain(|c| c.uuid != uuid);
    }
    pub fn remove_mix_by_uuid(&mut self, uuid: Uuid) {
        self.mixes.retain(|m| m.uuid != uuid);
    }
}
