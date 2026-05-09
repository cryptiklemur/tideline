use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Plugin-emitted UI tree. Opaque to the host -- Svelte renders it.
pub type UiNode = Value;

/// Plugin-emitted icon reference. Opaque to the host.
pub type UiIconRef = Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SettingsSectionContribution {
    pub plugin_id: String,
    pub surface_id: String,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<UiIconRef>,
    #[serde(default)]
    pub priority: i32,
    /// If set, this section is shown nested under the named surface_id of the
    /// same plugin rather than as a top-level sidebar entry.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_surface_id: Option<String>,
    pub tree: UiNode,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatusPillContribution {
    pub plugin_id: String,
    pub surface_id: String,
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tone: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<UiIconRef>,
    #[serde(default)]
    pub priority: i32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tooltip: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ChannelFilter {
    All,
    ChannelIds { ids: Vec<String> },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OverlayPlacement {
    Detail,
    SidebarBadge,
    HeaderChip,
    ChannelCard,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelOverlayContribution {
    pub plugin_id: String,
    pub surface_id: String,
    pub placement: OverlayPlacement,
    pub channel_filter: ChannelFilter,
    pub tree: UiNode,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum InputFilter {
    All,
    PhysicalOnly,
    SourceNames { names: Vec<String> },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InputOverlayContribution {
    pub plugin_id: String,
    pub surface_id: String,
    pub input_filter: InputFilter,
    pub tree: UiNode,
    #[serde(default)]
    pub values_by_source:
        std::collections::HashMap<String, std::collections::HashMap<String, serde_json::Value>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrayItemContribution {
    pub plugin_id: String,
    pub item_id: String,
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub accelerator: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<UiIconRef>,
    #[serde(default)]
    pub priority: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeybindActionContribution {
    pub plugin_id: String,
    pub action_id: String,
    pub label: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IframeSurface {
    pub plugin_id: String,
    pub surface_id: String,
    pub entry_path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub initial_data: Option<Value>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Contributions {
    #[serde(default)]
    pub settings_sections: Vec<SettingsSectionContribution>,
    #[serde(default)]
    pub status_pills: Vec<StatusPillContribution>,
    #[serde(default)]
    pub channel_overlays: Vec<ChannelOverlayContribution>,
    #[serde(default)]
    pub tray_items: Vec<TrayItemContribution>,
    #[serde(default)]
    pub keybind_actions: Vec<KeybindActionContribution>,
    #[serde(default)]
    pub iframe_surfaces: Vec<IframeSurface>,
    #[serde(default)]
    pub input_overlays: Vec<InputOverlayContribution>,
}
