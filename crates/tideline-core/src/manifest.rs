use crate::model::ChannelKind;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginManifest {
    pub id: String,
    pub name: String,
    pub version: String,
    pub entrypoint: Entrypoint,
    #[serde(default)]
    pub capabilities: Vec<String>,
    #[serde(default)]
    pub channel_overlays: Vec<ChannelOverlayDecl>,
    #[serde(default)]
    pub pipewire: Option<PipewireSettings>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entrypoint {
    pub command: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelOverlayDecl {
    pub id: String,
    pub scope: OverlayScope,
    pub surface: OverlaySurface,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum OverlayScope {
    AllChannels,
    AllMixes,
    ChannelKinds { kinds: Vec<ChannelKind> },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OverlaySurface {
    CardExtension,
    PanelSection,
    HeaderBadge,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipewireSettings {
    #[serde(default)]
    pub priority: i32,
}
