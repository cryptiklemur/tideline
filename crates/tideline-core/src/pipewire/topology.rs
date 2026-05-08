use super::directive::{ArgValue, LoadModuleHeader, PipewireDirective, RewireableTag};
use super::{fx_source_node, mix_capture_node, mix_playback_node, sink_node_for_channel};
use crate::config_io::slug;
use crate::model::{AppConfig, ChannelKind};

fn quoted(s: impl Into<String>) -> ArgValue {
    ArgValue::Quoted(s.into())
}

fn literal(s: impl Into<String>) -> ArgValue {
    ArgValue::Literal(s.into())
}

fn fl_fr() -> ArgValue {
    ArgValue::Quoted("FL,FR".into())
}

fn role_for_kind(kind: ChannelKind) -> &'static str {
    match kind {
        ChannelKind::Output => "output_loopback",
        ChannelKind::Input => "input_loopback",
        ChannelKind::PhysicalInput => "physical_input_loopback",
    }
}

fn loopback_directive(
    capture_props: Vec<(String, ArgValue)>,
    playback_props: Vec<(String, ArgValue)>,
    tag: RewireableTag,
) -> PipewireDirective {
    PipewireDirective::LoadModule {
        header: LoadModuleHeader::Named("libpipewire-module-loopback".into()),
        args: vec![
            ("capture.props".into(), ArgValue::Group(capture_props)),
            ("playback.props".into(), ArgValue::Group(playback_props)),
        ],
        rewireable_tag: Some(tag),
    }
}

pub fn build_base_topology(
    cfg: &AppConfig,
    _mix_mutes: &[super::directive::MixMuteEntry],
) -> Vec<PipewireDirective> {
    // Mute is no longer enforced at the conf level — post-loopbacks
    // always exist and mute is applied at runtime via
    // `set-sink-input-volume 0%` on the playback sink-input. Keeping
    // the parameter for signature compatibility with callers.
    let mut out: Vec<PipewireDirective> = Vec::new();

    for ch in &cfg.channels {
        if ch.kind == ChannelKind::PhysicalInput {
            continue;
        }
        let sink_name = sink_node_for_channel(ch);
        out.push(PipewireDirective::LoadModule {
            header: LoadModuleHeader::Factory("adapter".into()),
            args: vec![
                ("factory.name".into(), literal("support.null-audio-sink")),
                ("node.name".into(), quoted(sink_name)),
                ("node.description".into(), quoted(&ch.name)),
                ("media.class".into(), quoted("Audio/Sink")),
                ("audio.position".into(), fl_fr()),
            ],
            rewireable_tag: None,
        });
    }

    for ch in &cfg.channels {
        match ch.kind {
            ChannelKind::Output => {
                let sink_name = sink_node_for_channel(ch);
                for mix in &cfg.mixes {
                    for (i, target) in mix.sinks.iter().enumerate() {
                        let cap = mix_capture_node(ch, mix, i);
                        let pb = mix_playback_node(ch, mix, i);
                        let capture_props = vec![
                            ("node.name".into(), quoted(cap)),
                            ("target.object".into(), quoted(&sink_name)),
                            ("audio.position".into(), fl_fr()),
                            ("stream.dont-remix".into(), literal("true")),
                            ("stream.capture.sink".into(), literal("true")),
                        ];
                        let playback_props = vec![
                            ("node.name".into(), quoted(pb)),
                            ("target.object".into(), quoted(target)),
                            ("audio.position".into(), fl_fr()),
                        ];
                        out.push(loopback_directive(
                            capture_props,
                            playback_props,
                            RewireableTag {
                                channel_uuid: ch.uuid.to_string(),
                                role: role_for_kind(ch.kind).into(),
                            },
                        ));
                    }
                }
            }
            ChannelKind::Input => {
                let sink_name = sink_node_for_channel(ch);
                let s = slug(&ch.name);
                for (i, src) in ch.sources.iter().enumerate() {
                    let capture_props = vec![
                        ("node.name".into(), quoted(format!("capture.{s}-src-{i}"))),
                        ("target.object".into(), quoted(src)),
                        ("audio.position".into(), fl_fr()),
                        ("stream.dont-remix".into(), literal("true")),
                    ];
                    let playback_props = vec![
                        ("node.name".into(), quoted(format!("playback.{s}-src-{i}"))),
                        ("target.object".into(), quoted(&sink_name)),
                        ("audio.position".into(), fl_fr()),
                    ];
                    out.push(loopback_directive(
                        capture_props,
                        playback_props,
                        RewireableTag {
                            channel_uuid: ch.uuid.to_string(),
                            role: role_for_kind(ch.kind).into(),
                        },
                    ));
                }
            }
            ChannelKind::PhysicalInput => {
                if ch.physical_source.is_empty() {
                    continue;
                }
                let s = slug(&ch.name);
                let loopback_tag = RewireableTag {
                    channel_uuid: ch.uuid.to_string(),
                    role: role_for_kind(ch.kind).into(),
                };
                let virtual_source_tag = RewireableTag {
                    channel_uuid: ch.uuid.to_string(),
                    role: "physical_input_virtual_source".into(),
                };

                let virt_source = fx_source_node(ch);
                let virt_desc = format!("{} - FX", ch.name);

                // Persistent virtual-source endpoint. Apps record from this
                // (it's named "{name} - FX" in their selectors). Owned by a
                // separate role so the effects-plugin contributor's
                // DestroyModule { role: physical_input_loopback } leaves it
                // alone — without this, toggling the chain would tear down
                // and recreate the virtual source, dropping app connections.
                out.push(PipewireDirective::LoadModule {
                    header: LoadModuleHeader::Factory("adapter".into()),
                    args: vec![
                        ("factory.name".into(), literal("support.null-audio-sink")),
                        ("node.name".into(), quoted(&virt_source)),
                        ("node.description".into(), quoted(&virt_desc)),
                        ("media.class".into(), quoted("Audio/Source/Virtual")),
                        ("audio.position".into(), fl_fr()),
                        ("monitor.passthrough".into(), literal("true")),
                    ],
                    rewireable_tag: Some(virtual_source_tag),
                });

                // Feeding loopback: real mic → virtual source. Replaced by
                // the contributor's fx-virtual loopback when the chain is
                // active (real mic → carla → virtual source).
                let feed_cap = format!("capture.{s}-fx-feed");
                let feed_pb = format!("playback.{s}-fx-feed");
                out.push(loopback_directive(
                    vec![
                        ("node.name".into(), quoted(feed_cap)),
                        ("target.object".into(), quoted(&ch.physical_source)),
                        ("audio.position".into(), fl_fr()),
                        ("stream.dont-remix".into(), literal("true")),
                    ],
                    vec![
                        ("node.name".into(), quoted(feed_pb)),
                        ("target.object".into(), quoted(&virt_source)),
                        ("audio.position".into(), fl_fr()),
                    ],
                    loopback_tag.clone(),
                ));

                for mix in &cfg.mixes {
                    for (i, target) in mix.sinks.iter().enumerate() {
                        let cap = mix_capture_node(ch, mix, i);
                        let pb = mix_playback_node(ch, mix, i);
                        let capture_props = vec![
                            ("node.name".into(), quoted(cap)),
                            ("target.object".into(), quoted(&ch.physical_source)),
                            ("audio.position".into(), fl_fr()),
                            ("stream.dont-remix".into(), literal("true")),
                        ];
                        let playback_props = vec![
                            ("node.name".into(), quoted(pb)),
                            ("target.object".into(), quoted(target)),
                            ("audio.position".into(), fl_fr()),
                        ];
                        out.push(loopback_directive(
                            capture_props,
                            playback_props,
                            loopback_tag.clone(),
                        ));
                    }
                }
            }
        }
    }

    out
}
