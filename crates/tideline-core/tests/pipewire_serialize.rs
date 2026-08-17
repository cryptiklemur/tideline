use std::collections::HashSet;
use std::fs;
use std::path::Path;
use tideline_core::model::AppConfig;
use tideline_core::pipewire::directive::PipewireDirective;
use tideline_core::pipewire::serialize::serialize_directives;
use tideline_core::pipewire::topology::build_base_topology;

fn cfg_with_one_mix_and_output_channel() -> AppConfig {
    let mut cfg = AppConfig::default();
    let mut mix = tideline_core::model::Mix::new("default", "Default");
    mix.sinks.push("alsa_output.real_sink_a".into());
    cfg.mixes.push(mix);
    cfg.channels
        .push(tideline_core::model::ChannelCfg::new("Game"));
    cfg
}

#[test]
fn base_topology_emits_load_module_per_loopback() {
    let cfg = cfg_with_one_mix_and_output_channel();
    let directives = build_base_topology(&cfg, &[], None);
    let load_modules = directives
        .iter()
        .filter(|d| matches!(d, PipewireDirective::LoadModule { .. }))
        .count();
    assert!(
        load_modules >= 1,
        "must emit at least one LoadModule, got {load_modules}"
    );
}

#[test]
fn loopbacks_carry_rewireable_tag_when_eligible() {
    let cfg = cfg_with_one_mix_and_output_channel();
    let directives = build_base_topology(&cfg, &[], None);
    let tagged = directives.iter().any(|d| match d {
        PipewireDirective::LoadModule { rewireable_tag, .. } => rewireable_tag.is_some(),
        _ => false,
    });
    assert!(
        tagged,
        "channel loopback must be tagged for rewire eligibility"
    );
}

#[test]
fn absent_mix_sink_emits_no_loopback() {
    let mut cfg = cfg_with_one_mix_and_output_channel();
    cfg.mixes[0].sinks.push("wivrn.sink".into());
    let available: HashSet<String> = ["alsa_output.real_sink_a".to_string()]
        .into_iter()
        .collect();

    let conf = serialize_directives(&build_base_topology(&cfg, &[], Some(&available)));

    assert!(
        !conf.contains("wivrn.sink"),
        "loopback to an absent sink must not be emitted:\n{conf}"
    );
    assert!(conf.contains("alsa_output.real_sink_a"));
}

#[test]
fn absent_mix_sink_does_not_renumber_the_survivors() {
    let mut cfg = cfg_with_one_mix_and_output_channel();
    cfg.mixes[0].sinks.insert(0, "gone.sink".into());
    let available: HashSet<String> = ["alsa_output.real_sink_a".to_string()]
        .into_iter()
        .collect();

    let conf = serialize_directives(&build_base_topology(&cfg, &[], Some(&available)));

    // real_sink_a sits at index 1, so its nodes stay -1 even though index 0
    // dropped out. renumbering would rename nodes on every device change.
    assert!(conf.contains("playback.game-default-1"), "got:\n{conf}");
    assert!(!conf.contains("playback.game-default-0"));
}

#[test]
fn no_sink_list_means_no_filtering() {
    let mut cfg = cfg_with_one_mix_and_output_channel();
    cfg.mixes[0].sinks.push("wivrn.sink".into());

    let conf = serialize_directives(&build_base_topology(&cfg, &[], None));

    assert!(conf.contains("wivrn.sink"));
}

#[test]
fn contributor_loopback_to_absent_sink_is_dropped_too() {
    use tideline_core::pipewire::build_pipewire_conf;
    use tideline_core::pipewire::directive::{ArgValue, LoadModuleHeader, PipewireDirective};

    let mut cfg = cfg_with_one_mix_and_output_channel();
    cfg.mixes[0].sinks.push("wivrn.sink".into());
    let available: HashSet<String> = ["alsa_output.real_sink_a".to_string()]
        .into_iter()
        .collect();

    // what tideline-effects contributes for an fx-enabled channel: its own
    // loopback straight to the mix target, bypassing base topology.
    let contributed = vec![PipewireDirective::LoadModule {
        header: LoadModuleHeader::Named("libpipewire-module-loopback".into()),
        args: vec![
            (
                "capture.props".into(),
                ArgValue::Group(vec![(
                    "target.object".into(),
                    ArgValue::Quoted("tideline-fx-abc".into()),
                )]),
            ),
            (
                "playback.props".into(),
                ArgValue::Group(vec![(
                    "target.object".into(),
                    ArgValue::Quoted("wivrn.sink".into()),
                )]),
            ),
        ],
        rewireable_tag: None,
    }];

    let conf = build_pipewire_conf(&cfg, &[contributed], &[], Some(&available));

    assert!(
        !conf.contains("wivrn.sink"),
        "contributed loopback to an absent sink must be dropped:\n{conf}"
    );
}

#[test]
fn internal_fx_targets_survive_the_filter() {
    use tideline_core::pipewire::build_pipewire_conf;

    let cfg = cfg_with_one_mix_and_output_channel();
    let available: HashSet<String> = ["alsa_output.real_sink_a".to_string()]
        .into_iter()
        .collect();

    let conf = build_pipewire_conf(&cfg, &[], &[], Some(&available));

    // fx_source.* / sink.* never appear in mix.sinks, so they must not be
    // culled just for being absent from the pactl sink list.
    assert!(conf.contains("sink.game"), "got:\n{conf}");
}

#[test]
fn directives_serialize_byte_equivalent_to_legacy() {
    let cfg: AppConfig = serde_json::from_str(
        &fs::read_to_string(Path::new("tests/fixtures/baseline_input.json")).unwrap(),
    )
    .unwrap();
    let directives = build_base_topology(&cfg, &[], None);
    let actual = serialize_directives(&directives);
    let expected = fs::read_to_string(Path::new("tests/fixtures/baseline.conf")).unwrap();
    pretty_assertions::assert_eq!(actual, expected);
}
