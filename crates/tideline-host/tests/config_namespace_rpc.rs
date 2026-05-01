use serde_json::json;
use tideline_host::rpc::config::{NamespaceStore, handle_namespace_get, handle_namespace_set};

#[test]
fn set_then_get_roundtrips_within_namespace() {
    let mut store = NamespaceStore::default();
    let plugin_id = "io.tideline.silence";
    let res = handle_namespace_set(plugin_id, "io.tideline.silence", json!({"threshold": -40}), &mut store);
    assert!(res.is_ok());
    let got = handle_namespace_get(plugin_id, "io.tideline.silence", &store).unwrap();
    assert_eq!(got, json!({"threshold": -40}));
}

#[test]
fn writing_to_other_namespace_is_denied() {
    let mut store = NamespaceStore::default();
    let res = handle_namespace_set("io.tideline.a", "io.tideline.b", json!({}), &mut store);
    assert!(res.is_err(), "must reject cross-namespace writes");
}

#[test]
fn reading_other_namespace_returns_null() {
    let store = NamespaceStore::default();
    let got = handle_namespace_get("io.tideline.a", "io.tideline.b", &store).unwrap();
    assert!(got.is_null());
}
