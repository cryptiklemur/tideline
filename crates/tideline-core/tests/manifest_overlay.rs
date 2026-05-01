use tideline_core::manifest::{OverlayScope, OverlaySurface, PluginManifest};

#[test]
fn manifest_parses_channel_overlays() {
    let toml_src = r#"
id = "io.tideline.silence"
name = "Silence Sentinel"
version = "0.1.0"
entrypoint = { command = ["python", "main.py"] }

[[channel_overlays]]
id = "silence_toggle"
scope = { kind = "all_channels" }
surface = "card_extension"

[[channel_overlays]]
id = "vc_only"
scope = { kind = "channel_kinds", kinds = ["input"] }
surface = "panel_section"

[pipewire]
priority = 50
"#;
    let m: PluginManifest = toml::from_str(toml_src).unwrap();
    assert_eq!(m.channel_overlays.len(), 2);
    assert!(matches!(m.channel_overlays[0].scope, OverlayScope::AllChannels));
    assert!(matches!(m.channel_overlays[0].surface, OverlaySurface::CardExtension));
    if let OverlayScope::ChannelKinds { kinds } = &m.channel_overlays[1].scope {
        assert_eq!(kinds.len(), 1);
    } else {
        panic!("expected ChannelKinds");
    }
    assert!(matches!(m.channel_overlays[1].surface, OverlaySurface::PanelSection));
    assert_eq!(m.pipewire.as_ref().unwrap().priority, 50);
}

#[test]
fn manifest_without_overlays_is_valid() {
    let toml_src = r#"
id = "io.tideline.minimal"
name = "Minimal"
version = "0.0.1"
entrypoint = { command = ["python", "m.py"] }
"#;
    let m: PluginManifest = toml::from_str(toml_src).unwrap();
    assert!(m.channel_overlays.is_empty());
}
