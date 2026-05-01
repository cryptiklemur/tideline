use std::collections::HashSet;
use tideline_sdk::Capability;

#[derive(Debug, Clone, Default)]
pub struct CapabilitySet {
    set: HashSet<Capability>,
}

impl CapabilitySet {
    pub fn new(caps: impl IntoIterator<Item = Capability>) -> Self {
        Self { set: caps.into_iter().collect() }
    }

    pub fn has(&self, c: Capability) -> bool {
        self.set.contains(&c)
    }

    pub fn grant(&mut self, c: Capability) {
        self.set.insert(c);
    }

    pub fn revoke(&mut self, c: Capability) -> bool {
        self.set.remove(&c)
    }

    pub fn iter(&self) -> impl Iterator<Item = Capability> + '_ {
        self.set.iter().copied()
    }

    pub fn as_vec(&self) -> Vec<Capability> {
        let mut v: Vec<Capability> = self.set.iter().copied().collect();
        v.sort_by_key(|c| serde_json::to_string(c).unwrap_or_default());
        v
    }
}

pub fn required_capability_for(method: &str) -> Option<Capability> {
    use Capability::*;
    Some(match method {
        "host/initialize" => return None,
        "host/log.write" => LogWrite,
        "host/event.subscribe" => EventsSubscribe,
        "host/event.unsubscribe" => return None,
        "host/event.publish" => EventsPublish,
        "host/channel.list" | "host/channel.get" => ChannelRead,
        "host/channel.subscribe_meters" => ChannelSubscribe,
        "host/channel.create" => ChannelCreate,
        "host/channel.update" => ChannelWrite,
        "host/channel.attach_data" => ChannelAttachData,
        "host/mix.attach_data" => MixAttachData,
        "host/levels.read" => LevelsRead,
        "host/audio.play" => AudioPlay,
        "host/source.set_mute" => AudioMute,
        "host/sources.list" => AudioBackendStatus,
        "host/audio.position" => AudioPosition,
        "host/notify" | "host/notify.send" => TrayContribute,
        "host/ui.iframe.show" | "host/ui.iframe.hide" => UiIframe,
        "host/ui.channel_overlay.focus" => UiChannelOverlay,
        "host/keybind.register" | "host/keybind.unregister" => KeybindRegister,
        "host/pipewire.contribute" => PipewireContribute,
        "host/config.namespace.get" => ConfigNamespaceRead,
        "host/config.namespace.set" => ConfigNamespaceWrite,
        "host/config.read" => ConfigRead,
        "host/config.write" => ConfigWrite,
        "host/fs.read" => FsRead,
        "host/fs.write" => FsWrite,
        "host/net.http" => NetHttp,
        "host/process.spawn" => ProcessSpawn,
        "host/secrets.read" => SecretsRead,
        "host/secrets.write" => SecretsWrite,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initialize_needs_no_capability() {
        assert!(required_capability_for("host/initialize").is_none());
    }

    #[test]
    fn channel_list_requires_channel_read() {
        assert_eq!(required_capability_for("host/channel.list"), Some(Capability::ChannelRead));
    }

    #[test]
    fn unknown_method_returns_none_so_caller_can_404() {
        assert!(required_capability_for("host/totally.unknown").is_none());
    }

    #[test]
    fn capability_set_grant_and_revoke() {
        let mut s = CapabilitySet::new([Capability::ChannelRead]);
        assert!(s.has(Capability::ChannelRead));
        assert!(!s.has(Capability::ChannelWrite));
        s.grant(Capability::ChannelWrite);
        assert!(s.has(Capability::ChannelWrite));
        assert!(s.revoke(Capability::ChannelRead));
        assert!(!s.revoke(Capability::ChannelRead));
    }

    #[test]
    fn audio_play_requires_audio_play_capability() {
        // Reconciliation: canonical audio.play replaces wave-1-draft audio.tone + audio.sample.
        assert_eq!(required_capability_for("host/audio.play"), Some(Capability::AudioPlay));
    }

    #[test]
    fn notify_requires_tray_contribute() {
        // Reconciliation: tray.contribute replaces wave-1-draft tray.menu + tray.notify.
        assert_eq!(required_capability_for("host/notify"), Some(Capability::TrayContribute));
        assert_eq!(required_capability_for("host/notify.send"), Some(Capability::TrayContribute));
    }
}
