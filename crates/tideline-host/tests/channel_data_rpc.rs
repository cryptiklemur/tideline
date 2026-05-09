use serde_json::json;
use tideline_core::model::{AppConfig, ChannelCfg};
use tideline_host::rpc::channel::{handle_attach_data, handle_detach_data};

#[test]
fn attach_then_detach_roundtrips() {
    let mut cfg = AppConfig::default();
    cfg.channels.push(ChannelCfg::new("Game"));
    let uuid = cfg.channels[0].uuid.to_string();

    handle_attach_data(
        "io.tideline.silence",
        &uuid,
        "io.tideline.silence",
        json!({"on": true}),
        &mut cfg,
    )
    .unwrap();
    assert_eq!(
        cfg.channels[0].plugin_data.get("io.tideline.silence"),
        Some(&json!({"on": true}))
    );

    handle_detach_data(
        "io.tideline.silence",
        &uuid,
        "io.tideline.silence",
        &mut cfg,
    )
    .unwrap();
    assert!(!cfg.channels[0]
        .plugin_data
        .contains_key("io.tideline.silence"));
}

#[test]
fn cross_namespace_attach_is_denied() {
    let mut cfg = AppConfig::default();
    cfg.channels.push(ChannelCfg::new("Game"));
    let uuid = cfg.channels[0].uuid.to_string();
    let err = handle_attach_data("io.tideline.a", &uuid, "io.tideline.b", json!({}), &mut cfg);
    assert!(err.is_err());
}

#[test]
fn unknown_channel_is_error() {
    let mut cfg = AppConfig::default();
    let err = handle_attach_data(
        "io.tideline.a",
        "00000000-0000-0000-0000-000000000000",
        "io.tideline.a",
        json!({}),
        &mut cfg,
    );
    assert!(err.is_err());
}
