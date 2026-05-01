use std::sync::{Arc, Mutex};

use serde_json::json;
use tideline_host::PluginRegistry;

#[tokio::test]
async fn registry_routes_ui_event_to_plugin() {
    let registry = PluginRegistry::new_for_test();
    let saw: Arc<Mutex<Option<serde_json::Value>>> = Arc::new(Mutex::new(None));
    let captured = saw.clone();
    registry
        .set_dispatch_for_test("plug-a", move |method, params| {
            assert_eq!(method, "ui/event");
            *captured.lock().unwrap() = Some(params.clone());
            json!(null)
        })
        .await;

    registry
        .dispatch_event(
            "plug-a",
            "ui/event",
            json!({ "surface_id": "sec-1", "node_id": "n1", "value": { "type": "click" } }),
        )
        .await
        .unwrap();

    let v = saw.lock().unwrap().clone().unwrap();
    assert_eq!(v["surface_id"], "sec-1");
    assert_eq!(v["value"]["type"], "click");
}
