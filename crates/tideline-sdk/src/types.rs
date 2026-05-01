use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
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
}
