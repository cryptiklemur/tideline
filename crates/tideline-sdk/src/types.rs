use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Capability {
    #[serde(rename = "channel.read")]
    ChannelRead,
    #[serde(rename = "channel.write")]
    ChannelWrite,
    #[serde(rename = "channel.subscribe")]
    ChannelSubscribe,
    #[serde(rename = "channel.create")]
    ChannelCreate,
    #[serde(rename = "channel.attach_data")]
    ChannelAttachData,
    #[serde(rename = "mix.attach_data")]
    MixAttachData,

    #[serde(rename = "config.read")]
    ConfigRead,
    #[serde(rename = "config.write")]
    ConfigWrite,
    #[serde(rename = "config.namespace.read")]
    ConfigNamespaceRead,
    #[serde(rename = "config.namespace.write")]
    ConfigNamespaceWrite,

    #[serde(rename = "events.publish")]
    EventsPublish,
    #[serde(rename = "events.subscribe")]
    EventsSubscribe,

    #[serde(rename = "audio.play")]
    AudioPlay,
    #[serde(rename = "audio.mute")]
    AudioMute,
    #[serde(rename = "audio.backend_status")]
    AudioBackendStatus,
    #[serde(rename = "audio.position")]
    AudioPosition,

    #[serde(rename = "levels.read")]
    LevelsRead,

    #[serde(rename = "pipewire.contribute")]
    PipewireContribute,

    #[serde(rename = "ui.settings_section")]
    UiSettingsSection,
    #[serde(rename = "ui.status_pill")]
    UiStatusPill,
    #[serde(rename = "ui.channel_overlay")]
    UiChannelOverlay,
    #[serde(rename = "ui.iframe")]
    UiIframe,
    #[serde(rename = "ui.input_overlay")]
    UiInputOverlay,

    #[serde(rename = "tray.contribute")]
    TrayContribute,

    #[serde(rename = "keybind.register")]
    KeybindRegister,
    #[serde(rename = "portal.global_shortcuts")]
    PortalGlobalShortcuts,

    #[serde(rename = "hardware.usb")]
    HardwareUsb,
    #[serde(rename = "hardware.evdev")]
    HardwareEvdev,

    #[serde(rename = "fs.read")]
    FsRead,
    #[serde(rename = "fs.write")]
    FsWrite,
    #[serde(rename = "net.http")]
    NetHttp,
    #[serde(rename = "process.spawn")]
    ProcessSpawn,
    #[serde(rename = "secrets.read")]
    SecretsRead,
    #[serde(rename = "secrets.write")]
    SecretsWrite,
    #[serde(rename = "log.write")]
    LogWrite,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    pub plugin: ManifestPluginInfo,
    pub host: ManifestHost,
    pub entry: ManifestEntry,
    #[serde(default)]
    pub capabilities: ManifestCapabilities,
    #[serde(default)]
    pub contributes: ManifestContributes,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestPluginInfo {
    pub schema: u32,
    pub id: String,
    pub name: String,
    pub version: String,
    pub publisher: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestHost {
    pub api: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestEntry {
    pub exec: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ManifestCapabilities {
    #[serde(default)]
    pub required: Vec<Capability>,
    #[serde(default)]
    pub optional: Vec<Capability>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ManifestContributes {
    #[serde(default)]
    pub publishes_topics: Vec<String>,
    #[serde(default)]
    pub channel_overlays: Vec<ChannelOverlayDecl>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelOverlayDecl {
    pub slot: String,
    pub icon: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_via_serde() {
        let json = serde_json::to_string(&Capability::ChannelRead).unwrap();
        assert_eq!(json, "\"channel.read\"");
        let back: Capability = serde_json::from_str(&json).unwrap();
        assert_eq!(back, Capability::ChannelRead);
    }

    #[test]
    fn rejects_unknown_capability() {
        let err = serde_json::from_str::<Capability>("\"channel.delete\"");
        assert!(err.is_err());
    }

    #[test]
    fn all_variants_round_trip() {
        let cases: &[(Capability, &str)] = &[
            (Capability::ChannelRead, "channel.read"),
            (Capability::ChannelWrite, "channel.write"),
            (Capability::ChannelSubscribe, "channel.subscribe"),
            (Capability::ChannelCreate, "channel.create"),
            (Capability::ChannelAttachData, "channel.attach_data"),
            (Capability::MixAttachData, "mix.attach_data"),
            (Capability::ConfigRead, "config.read"),
            (Capability::ConfigWrite, "config.write"),
            (Capability::ConfigNamespaceRead, "config.namespace.read"),
            (Capability::ConfigNamespaceWrite, "config.namespace.write"),
            (Capability::EventsPublish, "events.publish"),
            (Capability::EventsSubscribe, "events.subscribe"),
            (Capability::AudioPlay, "audio.play"),
            (Capability::AudioMute, "audio.mute"),
            (Capability::AudioBackendStatus, "audio.backend_status"),
            (Capability::AudioPosition, "audio.position"),
            (Capability::LevelsRead, "levels.read"),
            (Capability::PipewireContribute, "pipewire.contribute"),
            (Capability::UiSettingsSection, "ui.settings_section"),
            (Capability::UiStatusPill, "ui.status_pill"),
            (Capability::UiChannelOverlay, "ui.channel_overlay"),
            (Capability::UiIframe, "ui.iframe"),
            (Capability::UiInputOverlay, "ui.input_overlay"),
            (Capability::TrayContribute, "tray.contribute"),
            (Capability::KeybindRegister, "keybind.register"),
            (Capability::PortalGlobalShortcuts, "portal.global_shortcuts"),
            (Capability::HardwareUsb, "hardware.usb"),
            (Capability::HardwareEvdev, "hardware.evdev"),
            (Capability::FsRead, "fs.read"),
            (Capability::FsWrite, "fs.write"),
            (Capability::NetHttp, "net.http"),
            (Capability::ProcessSpawn, "process.spawn"),
            (Capability::SecretsRead, "secrets.read"),
            (Capability::SecretsWrite, "secrets.write"),
            (Capability::LogWrite, "log.write"),
        ];

        assert_eq!(cases.len(), 35, "should cover every variant exactly once");

        for (variant, expected_wire) in cases {
            let json = serde_json::to_string(variant).unwrap();
            assert_eq!(
                json,
                format!("\"{}\"", expected_wire),
                "serialize mismatch for {:?}",
                variant
            );
            let back: Capability = serde_json::from_str(&json).unwrap();
            assert_eq!(back, *variant, "round-trip mismatch for {:?}", variant);
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PermissionsFile {
    pub plugin_id: String,
    pub granted: Vec<Capability>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionsChangedParams {
    pub granted: Vec<Capability>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioSource {
    pub name: String,
    pub description: String,
}

#[cfg(test)]
mod permissions_tests {
    use super::*;

    #[test]
    fn permissions_round_trip_toml() {
        let file = PermissionsFile {
            plugin_id: "io.tideline.test".into(),
            granted: vec![Capability::ChannelRead, Capability::EventsPublish],
        };
        let s = toml::to_string(&file).unwrap();
        let back: PermissionsFile = toml::from_str(&s).unwrap();
        assert_eq!(back.plugin_id, "io.tideline.test");
        assert_eq!(
            back.granted,
            vec![Capability::ChannelRead, Capability::EventsPublish]
        );
    }
}
