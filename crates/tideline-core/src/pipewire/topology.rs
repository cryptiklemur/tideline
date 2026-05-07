use super::directive::{ArgValue, LoadModuleHeader, PipewireDirective, RewireableTag};
use super::{mix_capture_node, mix_playback_node, sink_node_for_channel};
use crate::config_io::slug;
use crate::model::{AppConfig, ChannelKind};

fn quoted(s: impl Into<String>) -> ArgValue {
    ArgValue::Quoted(s.into())
}

fn literal(s: impl Into<String>) -> ArgValue {
    ArgValue::Literal(s.into())
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
    mix_mutes: &[super::directive::MixMuteEntry],
) -> Vec<PipewireDirective> {
    let mut out: Vec<PipewireDirective> = Vec::new();

    let is_muted = |channel_name: &str, mix_id: &str| -> bool {
        mix_mutes
            .iter()
            .any(|m| m.muted && m.channel_name == channel_name && m.mix_id == mix_id)
    };

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
                ("audio.position".into(), quoted("FL,FR")),
            ],
            rewireable_tag: None,
        });
    }

    for ch in &cfg.channels {
        match ch.kind {
            ChannelKind::Output => {
                let sink_name = sink_node_for_channel(ch);
                for mix in &cfg.mixes {
                    if is_muted(&ch.name, &mix.id) {
                        continue;
                    }
                    for (i, target) in mix.sinks.iter().enumerate() {
                        let cap = mix_capture_node(ch, mix, i);
                        let pb = mix_playback_node(ch, mix, i);
                        let capture_props = vec![
                            ("node.name".into(), quoted(cap)),
                            ("target.object".into(), quoted(&sink_name)),
                            ("audio.position".into(), quoted("FL,FR")),
                            ("stream.dont-remix".into(), literal("true")),
                            ("stream.capture.sink".into(), literal("true")),
                        ];
                        let playback_props = vec![
                            ("node.name".into(), quoted(pb)),
                            ("target.object".into(), quoted(target)),
                            ("audio.position".into(), quoted("FL,FR")),
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
                        ("audio.position".into(), quoted("FL,FR")),
                        ("stream.dont-remix".into(), literal("true")),
                    ];
                    let playback_props = vec![
                        ("node.name".into(), quoted(format!("playback.{s}-src-{i}"))),
                        ("target.object".into(), quoted(&sink_name)),
                        ("audio.position".into(), quoted("FL,FR")),
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
                let tag = RewireableTag {
                    channel_uuid: ch.uuid.to_string(),
                    role: role_for_kind(ch.kind).into(),
                };

                // Virtual FX source for this physical input. Apps (e.g.
                // Discord) can pick this as their input device. Without
                // an effects chain it passes physical_source through;
                // the effects contributor replaces capture target with
                // the fx node when a chain is present. Mute does NOT
                // affect the FX source — apps wiring to it always get
                // the channel's audio regardless of per-mix mute.
                let virt_cap = format!("capture.{s}-fx-virtual");
                let virt_pb = format!("playback.{s}-fx-source");
                let virt_desc = format!("{} - FX", ch.name);
                out.push(loopback_directive(
                    vec![
                        ("node.name".into(), quoted(virt_cap)),
                        ("target.object".into(), quoted(&ch.physical_source)),
                        ("audio.position".into(), quoted("FL,FR")),
                        ("stream.dont-remix".into(), literal("true")),
                    ],
                    vec![
                        ("node.name".into(), quoted(virt_pb)),
                        ("node.description".into(), quoted(&virt_desc)),
                        ("media.class".into(), quoted("Audio/Source/Virtual")),
                        ("audio.position".into(), quoted("FL,FR")),
                    ],
                    tag.clone(),
                ));

                for mix in &cfg.mixes {
                    if is_muted(&ch.name, &mix.id) {
                        continue;
                    }
                    for (i, target) in mix.sinks.iter().enumerate() {
                        let cap = mix_capture_node(ch, mix, i);
                        let pb = mix_playback_node(ch, mix, i);
                        let capture_props = vec![
                            ("node.name".into(), quoted(cap)),
                            ("target.object".into(), quoted(&ch.physical_source)),
                            ("audio.position".into(), quoted("FL,FR")),
                            ("stream.dont-remix".into(), literal("true")),
                        ];
                        let playback_props = vec![
                            ("node.name".into(), quoted(pb)),
                            ("target.object".into(), quoted(target)),
                            ("audio.position".into(), quoted("FL,FR")),
                        ];
                        out.push(loopback_directive(
                            capture_props,
                            playback_props,
                            tag.clone(),
                        ));
                    }
                }
            }
        }
    }

    out
}
