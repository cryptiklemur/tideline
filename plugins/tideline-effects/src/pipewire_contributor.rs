//! Builds pipewire directives that thread each channel's audio path through
//! the in-process audio engine's JACK client (`tideline-fx-{simple_uuid}`) when the channel has a
//! non-empty, non-bypassed effects chain.

use std::sync::Arc;

use serde_json::Value;
use tideline_core::config_io::slug;
use tideline_core::model::{AppConfig, ChannelCfg, ChannelKind, Mix};
use tideline_core::pipewire::directive::{ArgValue, LoadModuleHeader, PipewireDirective, RewireableTag};
use tideline_core::pipewire::{fx_source_node, mix_capture_node, mix_playback_node, sink_node_for_channel};
use tideline_sdk::contribute::{MixMuteEntry, PipewireContributeRequest};
use tideline_sdk::rpc::{error_codes, RpcError};

use crate::effect::{ChannelEffectsData, Effect};
use crate::state::EffectsState;
use crate::util::channel_jack_client;

pub async fn respond(state: &Arc<EffectsState>, params: Option<Value>) -> Result<Value, RpcError> {
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
    let cfg: tideline_core::model::AppConfig = serde_json::from_value(req.config.json.clone())
        .map_err(|e| RpcError {
            code: error_codes::INVALID_PARAMS,
            message: format!("pipewire.contribute_request: cfg parse failed: {e}"),
            data: None,
        })?;

    // First-call sync: rebuild engine chains from AppConfig.plugin_data so a
    // missing/stale chains.json gets superseded by the host's source-of-truth.
    // Wait for the catalog before reconciling — contribute_request can arrive
    // before `discovery::run_first_boot` populates it, in which case
    // `add_effect` fails with "plugin <uri> not in catalog" for every effect.
    if !state
        .appconfig_synced
        .swap(true, std::sync::atomic::Ordering::Relaxed)
    {
        state.wait_for_catalog().await;
        let persisted = persisted_chains_from_appconfig(&cfg);
        let total = persisted.channels.len();
        if total > 0 {
            tracing::info!(
                target: "tideline-effects::pipewire_contributor",
                channels = total,
                "first contribute: reconciling engine state from AppConfig.plugin_data"
            );
            crate::state::apply_persisted_chains(state.clone(), persisted).await;
        }
    }

    let mix_mutes = req.mix_mutes;
    let dirs = build_all_directives(&cfg, &mix_mutes);
    let channels_with_fx: Vec<String> = cfg
        .channels
        .iter()
        .filter(|c| {
            c.plugin_data
                .get("tideline-effects")
                .and_then(|v| v.get("effects"))
                .and_then(|e| e.as_array())
                .map(|a| !a.is_empty())
                .unwrap_or(false)
        })
        .map(|c| c.name.clone())
        .collect();
    let muted_pairs: Vec<String> = mix_mutes
        .iter()
        .filter(|m| m.muted)
        .map(|m| format!("{}:{}", m.channel_name, m.mix_id))
        .collect();
    tracing::info!(
        target: "tideline-effects::pipewire_contributor",
        channels_total = cfg.channels.len(),
        channels_with_fx = ?channels_with_fx,
        muted_mix_pairs = ?muted_pairs,
        directive_count = dirs.len(),
        "respond: built directives"
    );
    let resp = tideline_sdk::contribute::PipewireContributeResponse { directives: dirs };
    serde_json::to_value(resp).map_err(|e| RpcError {
        code: error_codes::INTERNAL_ERROR,
        message: format!("pipewire.contribute_request: serialize: {e}"),
        data: None,
    })
}

fn persisted_chains_from_appconfig(cfg: &AppConfig) -> crate::persist::PersistedChains {
    use std::collections::BTreeMap;
    let mut channels = BTreeMap::new();
    for ch in &cfg.channels {
        let Some(raw) = ch.plugin_data.get("tideline-effects") else {
            continue;
        };
        let data: ChannelEffectsData = match serde_json::from_value(raw.clone()) {
            Ok(d) => d,
            Err(_) => continue,
        };
        if data.effects.is_empty() {
            continue;
        }
        channels.insert(
            ch.uuid,
            crate::persist::PersistedChannel {
                bypassed: data.chain_bypassed,
                effects: data.effects,
            },
        );
    }
    crate::persist::PersistedChains {
        version: 2,
        channels,
    }
}

pub fn build_all_directives(
    cfg: &AppConfig,
    mix_mutes: &[MixMuteEntry],
) -> Vec<PipewireDirective> {
    let mut out = Vec::new();
    for ch in &cfg.channels {
        let data = read_effects_data(ch);
        if !chain_should_apply(&data) {
            continue;
        }
        out.extend(build_directives_for_channel(
            ch,
            &cfg.mixes,
            &data,
            mix_mutes,
        ));
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

pub fn build_directives_for_channel(
    ch: &ChannelCfg,
    mixes: &[Mix],
    data: &ChannelEffectsData,
    _mix_mutes: &[MixMuteEntry],
) -> Vec<PipewireDirective> {
    // Mute is no longer enforced at the conf level — post-loopbacks
    // always exist and mute is applied at runtime via
    // `set-sink-input-volume 0%` on the playback sink-input. Keeping
    // the parameter for signature compatibility with callers.
    let tag = RewireableTag {
        channel_uuid: ch.uuid.to_string(),
        role: role_for_kind(ch.kind).into(),
    };

    if matches!(ch.kind, ChannelKind::PhysicalInput) && ch.physical_source.is_empty() {
        return Vec::new();
    }

    let chain: Vec<&Effect> = data.effects.iter().filter(|e| !e.bypassed).collect();
    if chain.is_empty() {
        return vec![PipewireDirective::DestroyModule { target_tag: tag }];
    }

    let s = slug(&ch.name);
    let fx_node = channel_jack_client(ch.uuid);

    let mut out = Vec::new();
    out.push(PipewireDirective::DestroyModule {
        target_tag: tag.clone(),
    });

    match ch.kind {
        ChannelKind::Output => {
            let sink_name = sink_node_for_channel(ch);
            out.push(loopback(
                vec![
                    ("node.name".into(), quoted(format!("capture.{s}-fx-pre"))),
                    ("target.object".into(), quoted(&sink_name)),
                    ("audio.position".into(), fl_fr()),
                    ("stream.dont-remix".into(), literal("true")),
                    ("stream.capture.sink".into(), literal("true")),
                ],
                vec![
                    ("node.name".into(), quoted(format!("playback.{s}-fx-pre"))),
                    ("target.object".into(), quoted(&fx_node)),
                    ("node.autoconnect".into(), literal("false")),
                    ("audio.position".into(), fl_fr()),
                ],
                Some(tag.clone()),
            ));
        }
        ChannelKind::Input => {
            for (i, src) in ch.sources.iter().enumerate() {
                out.push(loopback(
                    vec![
                        ("node.name".into(), quoted(format!("capture.{s}-fx-src-{i}"))),
                        ("target.object".into(), quoted(src)),
                        ("audio.position".into(), fl_fr()),
                        ("stream.dont-remix".into(), literal("true")),
                    ],
                    vec![
                        ("node.name".into(), quoted(format!("playback.{s}-fx-src-{i}"))),
                        ("target.object".into(), quoted(&fx_node)),
                        ("node.autoconnect".into(), literal("false")),
                        ("audio.position".into(), fl_fr()),
                    ],
                    Some(tag.clone()),
                ));
            }
        }
        ChannelKind::PhysicalInput => {
            out.push(loopback(
                vec![
                    ("node.name".into(), quoted(format!("capture.{s}-fx-pre"))),
                    ("target.object".into(), quoted(&ch.physical_source)),
                    ("audio.position".into(), fl_fr()),
                    ("stream.dont-remix".into(), literal("true")),
                ],
                vec![
                    ("node.name".into(), quoted(format!("playback.{s}-fx-pre"))),
                    ("target.object".into(), quoted(&fx_node)),
                    ("node.autoconnect".into(), literal("false")),
                    ("audio.position".into(), fl_fr()),
                ],
                Some(tag.clone()),
            ));
        }
    }

    match ch.kind {
        ChannelKind::Output | ChannelKind::PhysicalInput => {
            if matches!(ch.kind, ChannelKind::PhysicalInput) {
                // Replaces the base topology's fx-feed loopback. Both
                // target the persistent virtual-source endpoint
                // `fx_source.{slug}` (created in topology.rs with role
                // `physical_input_virtual_source` so it survives this
                // contributor's DestroyModule). Capture side reads from
                // the in-process JACK client (livi-hosted LV2 chain);
                // playback writes into the virtual source, where apps
                // record from.
                let virt_cap = format!("capture.{s}-fx-virtual");
                let virt_pb = format!("playback.{s}-fx-virtual");
                let virt_source = fx_source_node(ch);
                out.push(loopback(
                    vec![
                        ("node.name".into(), quoted(virt_cap)),
                        ("target.object".into(), quoted(&fx_node)),
                        ("node.autoconnect".into(), literal("false")),
                        ("audio.position".into(), fl_fr()),
                        ("stream.dont-remix".into(), literal("true")),
                    ],
                    vec![
                        ("node.name".into(), quoted(virt_pb)),
                        ("target.object".into(), quoted(&virt_source)),
                        ("node.autoconnect".into(), literal("false")),
                        ("audio.position".into(), fl_fr()),
                    ],
                    Some(tag.clone()),
                ));
            }

            for mix in mixes {
                for (i, target) in mix.sinks.iter().enumerate() {
                    let cap = mix_capture_node(ch, mix, i);
                    let pb = mix_playback_node(ch, mix, i);
                    out.push(loopback(
                        vec![
                            ("node.name".into(), quoted(cap)),
                            ("target.object".into(), quoted(&fx_node)),
                            ("node.autoconnect".into(), literal("false")),
                            ("audio.position".into(), fl_fr()),
                            ("stream.dont-remix".into(), literal("true")),
                        ],
                        vec![
                            ("node.name".into(), quoted(pb)),
                            ("target.object".into(), quoted(target)),
                            ("audio.position".into(), fl_fr()),
                        ],
                        Some(tag.clone()),
                    ));
                }
            }
        }
        ChannelKind::Input => {
            let sink_name = sink_node_for_channel(ch);
            out.push(loopback(
                vec![
                    ("node.name".into(), quoted(format!("capture.{s}-fx-post"))),
                    ("target.object".into(), quoted(&fx_node)),
                    ("node.autoconnect".into(), literal("false")),
                    ("audio.position".into(), fl_fr()),
                    ("stream.dont-remix".into(), literal("true")),
                ],
                vec![
                    ("node.name".into(), quoted(format!("playback.{s}-fx-post"))),
                    ("target.object".into(), quoted(&sink_name)),
                    ("audio.position".into(), fl_fr()),
                ],
                Some(tag.clone()),
            ));
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

fn fl_fr() -> ArgValue {
    ArgValue::Quoted("FL,FR".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::effect::PluginFormat;
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

    fn make_effect(id: Uuid, uri: &str) -> Effect {
        Effect {
            id,
            format: PluginFormat::Lv2,
            uri: uri.into(),
            display_name: "Effect".into(),
            bypassed: false,
            state_b64: None,
        }
    }

    fn one_effect_data() -> ChannelEffectsData {
        ChannelEffectsData {
            effects: vec![make_effect(Uuid::nil(), "http://lsp-plug.in/plugins/lv2/gate_mono")],
            chain_bypassed: false,
        }
    }

    fn three_effect_data() -> ChannelEffectsData {
        ChannelEffectsData {
            effects: vec![
                make_effect(Uuid::from_u128(1), "http://lsp-plug.in/plugins/lv2/gate_stereo"),
                make_effect(Uuid::from_u128(2), "http://lsp-plug.in/plugins/lv2/para_equalizer_x16_stereo"),
                make_effect(Uuid::from_u128(3), "http://lsp-plug.in/plugins/lv2/sc_compressor_stereo"),
            ],
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
        let dirs = build_all_directives(&cfg, &[]);
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
        assert!(build_all_directives(&cfg, &[]).is_empty());
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

        let dirs = build_all_directives(&cfg, &[]);
        // destroy + pre + 2 post (one per mix sink); 1-effect chain has no inter-links
        assert_eq!(dirs.len(), 4);
        assert!(matches!(dirs[0], PipewireDirective::DestroyModule { .. }));
        for d in &dirs[1..] {
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
        let dirs = build_all_directives(&cfg, &[]);
        // destroy + 2 src pre-loopbacks + 1 post (1-effect chain, no inter-links)
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
        assert_eq!(build_all_directives(&cfg, &[]).len(), 0);
    }

    #[test]
    fn three_effect_chain_emits_one_pre_and_one_post() {
        // The new in-process engine runs the entire chain inside a single
        // JACK client per channel, so there are no inter-effect loopbacks
        // regardless of chain length.
        let ch = output_channel();
        let mut cfg = AppConfig::default();
        let mut ch2 = ch;
        ch2.plugin_data.insert(
            "tideline-effects".into(),
            serde_json::to_value(three_effect_data()).unwrap(),
        );
        cfg.channels.push(ch2);
        cfg.mixes.push(one_mix("default_sink"));

        let dirs = build_all_directives(&cfg, &[]);
        // destroy + pre + 1 post = 3
        assert_eq!(dirs.len(), 3);
        assert!(matches!(dirs[0], PipewireDirective::DestroyModule { .. }));
    }

    #[test]
    fn bypassed_effect_skipped_from_chain() {
        // Bypass affects in-process plugin processing (handled by the engine);
        // pipewire routing is still pre + post regardless. The chain remains
        // present as long as at least one effect is non-bypassed.
        let ch = output_channel();
        let mut data = three_effect_data();
        data.effects[1].bypassed = true; // middle one bypassed
        let mut cfg = AppConfig::default();
        let mut ch2 = ch;
        ch2.plugin_data
            .insert("tideline-effects".into(), serde_json::to_value(data).unwrap());
        cfg.channels.push(ch2);
        cfg.mixes.push(one_mix("default_sink"));

        let dirs = build_all_directives(&cfg, &[]);
        // destroy + pre + 1 post = 3
        assert_eq!(dirs.len(), 3);
    }

    #[test]
    fn all_effects_bypassed_emits_only_destroy() {
        let ch = output_channel();
        let mut data = one_effect_data();
        data.effects[0].bypassed = true;
        let mut cfg = AppConfig::default();
        let mut ch2 = ch;
        ch2.plugin_data
            .insert("tideline-effects".into(), serde_json::to_value(data).unwrap());
        cfg.channels.push(ch2);
        cfg.mixes.push(one_mix("default_sink"));

        let dirs = build_all_directives(&cfg, &[]);
        assert_eq!(dirs.len(), 1);
        assert!(matches!(dirs[0], PipewireDirective::DestroyModule { .. }));
    }
}
