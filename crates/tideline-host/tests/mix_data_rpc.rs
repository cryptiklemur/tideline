use serde_json::json;
use tideline_core::model::{AppConfig, Mix};
use tideline_host::rpc::mix::{handle_attach_data, handle_detach_data};

#[test]
fn attach_detach_roundtrips_on_mix() {
    let mut cfg = AppConfig::default();
    cfg.mixes.push(Mix::new("default", "Default"));
    let uuid = cfg.mixes[0].uuid.to_string();

    handle_attach_data("io.tideline.silence", &uuid, "io.tideline.silence", json!({"k": 1}), &mut cfg).unwrap();
    assert_eq!(cfg.mixes[0].plugin_data.get("io.tideline.silence"), Some(&json!({"k": 1})));

    handle_detach_data("io.tideline.silence", &uuid, "io.tideline.silence", &mut cfg).unwrap();
    assert!(!cfg.mixes[0].plugin_data.contains_key("io.tideline.silence"));
}

#[test]
fn cross_namespace_denied() {
    let mut cfg = AppConfig::default();
    cfg.mixes.push(Mix::new("default", "Default"));
    let uuid = cfg.mixes[0].uuid.to_string();
    let err = handle_attach_data("io.tideline.a", &uuid, "io.tideline.b", json!({}), &mut cfg);
    assert!(err.is_err());
}
