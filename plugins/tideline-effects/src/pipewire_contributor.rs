//! Builds pipewire directives that thread each channel's audio path through
//! a carla JACK client (`tideline-fx-{uuid8}`) when the channel has a
//! non-empty, non-bypassed effects chain.

use std::sync::Arc;

use serde_json::Value;
use tideline_core::config_io::slug;
use tideline_core::model::{AppConfig, ChannelCfg, ChannelKind, Mix};
use tideline_core::pipewire::directive::{ArgValue, LoadModuleHeader, PipewireDirective, RewireableTag};
use tideline_core::pipewire::{mix_capture_node, mix_playback_node, sink_node_for_channel};
use tideline_sdk::contribute::{on_pipewire_contribute, PipewireContributeRequest};
use tideline_sdk::rpc::{error_codes, RpcError};

use crate::effect::ChannelEffectsData;
use crate::state::EffectsState;
use crate::util::{fx_input_port, fx_output_port};

/// Top-level RPC entry. Parses request, walks channels, builds directives.
pub async fn respond(_state: &Arc<EffectsState>, params: Option<Value>) -> Result<Value, RpcError> {
    let raw = params.ok_or_else(|| RpcError {
        code: error_codes::INVALID_PARAMS,
        message: "pipewire.contribute_request: missing params".into(),
        data: None,
    })?;
    let req: PipewireContributeRequest = serde_json::from_value(raw).map_err(|e| RpcError {
        code: error_codes::INVALID_PARAMS,
        message: format!("pipewire.contribute_request: parse failed: {e}"),
        data: None,
    })?;
    let resp = on_pipewire_contribute(req, |cfg| Ok(build_all_directives(&cfg))).map_err(|e| {
        RpcError {
            code: error_codes::INTERNAL_ERROR,
            message: format!("pipewire.contribute_request: {e}"),
            data: None,
        }
    })?;
    serde_json::to_value(resp).map_err(|e| RpcError {
        code: error_codes::INTERNAL_ERROR,
        message: format!("pipewire.contribute_request: serialize: {e}"),
        data: None,
    })
}

/// Walks every channel, calls per-channel builder for those with effects.
pub fn build_all_directives(cfg: &AppConfig) -> Vec<PipewireDirective> {
    let mut out = Vec::new();
    for ch in &cfg.channels {
        let data = read_effects_data(ch);
        if !chain_should_apply(&data) {
            continue;
        }
        out.extend(build_directives_for_channel(ch, &cfg.mixes, &data));
    }
    out
}

fn chain_should_apply(data: &ChannelEffectsData) -> bool {
    !data.effects.is_empty() && !data.chain_bypassed
}

fn read_effects_data(ch: &ChannelCfg) -> ChannelEffectsData {
    ch.plugin_data
        .get("tideline-effects")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default()
}

fn role_for_kind(kind: ChannelKind) -> &'static str {
    match kind {
        ChannelKind::Output => "output_loopback",
        ChannelKind::Input => "input_loopback",
        ChannelKind::PhysicalInput => "physical_input_loopback",
    }
}

/// Builds directives for one channel with a non-empty chain:
///   - 1× `DestroyModule` for the channel's existing loopbacks (drops ALL by tag)
///   - LoadModule(s) that wire `sink/source ↔ carla in/out ↔ mix-target/sink`
pub fn build_directives_for_channel(
    ch: &ChannelCfg,
    mixes: &[Mix],
    _data: &ChannelEffectsData,
) -> Vec<PipewireDirective> {
    let tag = RewireableTag {
        channel_uuid: ch.uuid.to_string(),
        role: role_for_kind(ch.kind).into(),
    };

    if matches!(ch.kind, ChannelKind::PhysicalInput) && ch.physical_source.is_empty() {
        return Vec::new();
    }

    let mut out = Vec::new();
    out.push(PipewireDirective::DestroyModule {
        target_tag: tag.clone(),
    });

    match ch.kind {
        ChannelKind::Output => {
            let sink_name = sink_node_for_channel(ch);
            out.push(loopback(
                vec![
                    ("node.name".into(), quoted(format!("capture.{}-fx-pre", slug(&ch.name)))),
                    ("target.object".into(), quoted(&sink_name)),
                    ("audio.position".into(), quoted("FL,FR")),
                    ("stream.dont-remix".into(), literal("true")),
                    ("stream.capture.sink".into(), literal("true")),
                ],
                vec![
                    ("node.name".into(), quoted(format!("playback.{}-fx-pre-l", slug(&ch.name)))),
                    ("target.object".into(), quoted(fx_input_port(ch.uuid, 'l'))),
                    ("audio.position".into(), quoted("FL,FR")),
                ],
                None,
            ));
            for mix in mixes {
                for (i, target) in mix.sinks.iter().enumerate() {
                    let cap = mix_capture_node(ch, mix, i);
                    let pb = mix_playback_node(ch, mix, i);
                    out.push(loopback(
                        vec![
                            ("node.name".into(), quoted(cap)),
                            ("target.object".into(), quoted(fx_output_port(ch.uuid, 'l'))),
                            ("audio.position".into(), quoted("FL,FR")),
                            ("stream.dont-remix".into(), literal("true")),
                        ],
                        vec![
                            ("node.name".into(), quoted(pb)),
                            ("target.object".into(), quoted(target)),
                            ("audio.position".into(), quoted("FL,FR")),
                        ],
                        Some(tag.clone()),
                    ));
                }
            }
        }
        ChannelKind::Input => {
            let sink_name = sink_node_for_channel(ch);
            let s = slug(&ch.name);
            for (i, src) in ch.sources.iter().enumerate() {
                out.push(loopback(
                    vec![
                        ("node.name".into(), quoted(format!("capture.{s}-fx-src-{i}"))),
                        ("target.object".into(), quoted(src)),
                        ("audio.position".into(), quoted("FL,FR")),
                        ("stream.dont-remix".into(), literal("true")),
                    ],
                    vec![
                        ("node.name".into(), quoted(format!("playback.{s}-fx-src-{i}"))),
                        ("target.object".into(), quoted(fx_input_port(ch.uuid, 'l'))),
                        ("audio.position".into(), quoted("FL,FR")),
                    ],
                    None,
                ));
            }
            out.push(loopback(
                vec![
                    ("node.name".into(), quoted(format!("capture.{s}-fx-post"))),
                    ("target.object".into(), quoted(fx_output_port(ch.uuid, 'l'))),
                    ("audio.position".into(), quoted("FL,FR")),
                    ("stream.dont-remix".into(), literal("true")),
                ],
                vec![
                    ("node.name".into(), quoted(format!("playback.{s}-fx-post"))),
                    ("target.object".into(), quoted(&sink_name)),
                    ("audio.position".into(), quoted("FL,FR")),
                ],
                Some(tag.clone()),
            ));
        }
        ChannelKind::PhysicalInput => {
            let s = slug(&ch.name);
            out.push(loopback(
                vec![
                    ("node.name".into(), quoted(format!("capture.{s}-fx-pre"))),
                    ("target.object".into(), quoted(&ch.physical_source)),
                    ("audio.position".into(), quoted("FL,FR")),
                    ("stream.dont-remix".into(), literal("true")),
                ],
                vec![
                    ("node.name".into(), quoted(format!("playback.{s}-fx-pre"))),
                    ("target.object".into(), quoted(fx_input_port(ch.uuid, 'l'))),
                    ("audio.position".into(), quoted("FL,FR")),
                ],
                None,
            ));
            for mix in mixes {
                for (i, target) in mix.sinks.iter().enumerate() {
                    let cap = mix_capture_node(ch, mix, i);
                    let pb = mix_playback_node(ch, mix, i);
                    out.push(loopback(
                        vec![
                            ("node.name".into(), quoted(cap)),
                            ("target.object".into(), quoted(fx_output_port(ch.uuid, 'l'))),
                            ("audio.position".into(), quoted("FL,FR")),
                            ("stream.dont-remix".into(), literal("true")),
                        ],
                        vec![
                            ("node.name".into(), quoted(pb)),
                            ("target.object".into(), quoted(target)),
                            ("audio.position".into(), quoted("FL,FR")),
                        ],
                        Some(tag.clone()),
                    ));
                }
            }
        }
    }
    out
}

fn loopback(
    capture_props: Vec<(String, ArgValue)>,
    playback_props: Vec<(String, ArgValue)>,
    rewireable_tag: Option<RewireableTag>,
) -> PipewireDirective {
    PipewireDirective::LoadModule {
        header: LoadModuleHeader::Named("libpipewire-module-loopback".into()),
        args: vec![
            ("capture.props".into(), ArgValue::Group(capture_props)),
            ("playback.props".into(), ArgValue::Group(playback_props)),
        ],
        rewireable_tag,
    }
}

fn quoted(s: impl Into<String>) -> ArgValue {
    ArgValue::Quoted(s.into())
}
fn literal(s: impl Into<String>) -> ArgValue {
    ArgValue::Literal(s.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::effect::{Effect, PluginFormat};
    use uuid::Uuid;

    fn output_channel() -> ChannelCfg {
        let mut ch = ChannelCfg::new("Voice");
        ch.uuid = Uuid::parse_str("12345678-9abc-def0-1234-56789abcdef0").unwrap();
        ch.kind = ChannelKind::Output;
        ch
    }

    fn one_mix(sink: &str) -> Mix {
        let mut m = Mix::new("main", "Main");
        m.sinks = vec![sink.into()];
        m
    }

    fn one_effect_data() -> ChannelEffectsData {
        ChannelEffectsData {
            effects: vec![Effect {
                id: Uuid::nil(),
                format: PluginFormat::Lv2,
                uri: "http://lsp-plug.in/plugins/lv2/gate_mono".into(),
                display_name: "Gate".into(),
                bypassed: false,
                state_b64: None,
            }],
            chain_bypassed: false,
        }
    }

    #[test]
    fn empty_chain_yields_no_directives() {
        let ch = output_channel();
        let mut cfg = AppConfig::default();
        cfg.channels.push(ch);
        cfg.mixes
            .push(one_mix("alsa_output.pci-0000_00_1f.3.analog-stereo"));
        let dirs = build_all_directives(&cfg);
        assert!(dirs.is_empty());
    }

    #[test]
    fn chain_bypassed_yields_no_directives() {
        let ch = output_channel();
        let mut cfg = AppConfig::default();
        let mut data = one_effect_data();
        data.chain_bypassed = true;
        let mut ch2 = ch;
        ch2.plugin_data
            .insert("tideline-effects".into(), serde_json::to_value(data).unwrap());
        cfg.channels.push(ch2);
        cfg.mixes.push(one_mix("default"));
        assert!(build_all_directives(&cfg).is_empty());
    }

    #[test]
    fn output_channel_emits_destroy_then_pre_then_post_per_mix_sink() {
        let ch = output_channel();
        let mut cfg = AppConfig::default();
        let mut ch2 = ch;
        ch2.plugin_data.insert(
            "tideline-effects".into(),
            serde_json::to_value(one_effect_data()).unwrap(),
        );
        cfg.channels.push(ch2);
        cfg.mixes.push(one_mix("alsa_output.target-a"));
        cfg.mixes.push({
            let mut m = Mix::new("hp", "Headphones");
            m.sinks = vec!["hp_out".into()];
            m
        });

        let dirs = build_all_directives(&cfg);
        assert_eq!(dirs.len(), 4);
        assert!(matches!(dirs[0], PipewireDirective::DestroyModule { .. }));
        assert!(matches!(
            dirs[1],
            PipewireDirective::LoadModule {
                rewireable_tag: None,
                ..
            }
        ));
        for d in &dirs[2..] {
            assert!(matches!(
                d,
                PipewireDirective::LoadModule {
                    rewireable_tag: Some(_),
                    ..
                }
            ));
        }
    }

    #[test]
    fn input_channel_emits_destroy_then_one_per_source_then_one_post() {
        let mut ch = ChannelCfg::new("Mic");
        ch.uuid = Uuid::parse_str("12345678-9abc-def0-1234-56789abcdef0").unwrap();
        ch.kind = ChannelKind::Input;
        ch.sources = vec!["alsa_in.0".into(), "alsa_in.1".into()];
        ch.plugin_data.insert(
            "tideline-effects".into(),
            serde_json::to_value(one_effect_data()).unwrap(),
        );
        let mut cfg = AppConfig::default();
        cfg.channels.push(ch);
        let dirs = build_all_directives(&cfg);
        assert_eq!(dirs.len(), 4);
        assert!(matches!(dirs[0], PipewireDirective::DestroyModule { .. }));
    }

    #[test]
    fn physical_input_with_empty_source_emits_nothing() {
        let mut ch = ChannelCfg::new("Hardware");
        ch.uuid = Uuid::parse_str("12345678-9abc-def0-1234-56789abcdef0").unwrap();
        ch.kind = ChannelKind::PhysicalInput;
        ch.physical_source = String::new();
        ch.plugin_data.insert(
            "tideline-effects".into(),
            serde_json::to_value(one_effect_data()).unwrap(),
        );
        let mut cfg = AppConfig::default();
        cfg.channels.push(ch);
        cfg.mixes.push(one_mix("default"));
        assert_eq!(build_all_directives(&cfg).len(), 0);
    }
}
