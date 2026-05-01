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
    cfg.channels.push(tideline_core::model::ChannelCfg::new("Game"));
    cfg
}

#[test]
fn base_topology_emits_load_module_per_loopback() {
    let cfg = cfg_with_one_mix_and_output_channel();
    let directives = build_base_topology(&cfg);
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
    let directives = build_base_topology(&cfg);
    let tagged = directives.iter().any(|d| match d {
        PipewireDirective::LoadModule { rewireable_tag, .. } => rewireable_tag.is_some(),
        _ => false,
    });
    assert!(tagged, "channel loopback must be tagged for rewire eligibility");
}

#[test]
fn directives_serialize_byte_equivalent_to_legacy() {
    let cfg: AppConfig = serde_json::from_str(
        &fs::read_to_string(Path::new("tests/fixtures/baseline_input.json")).unwrap(),
    )
    .unwrap();
    let directives = build_base_topology(&cfg);
    let actual = serialize_directives(&directives);
    let expected = fs::read_to_string(Path::new("tests/fixtures/baseline.conf")).unwrap();
    pretty_assertions::assert_eq!(actual, expected);
}
