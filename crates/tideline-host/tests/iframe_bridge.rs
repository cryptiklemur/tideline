use serde_json::json;
use std::sync::{Arc, Mutex};
use tideline_host::{IframeBridge, PluginRegistry};

#[tokio::test]
async fn iframe_send_stamps_origin_and_forwards_to_plugin() {
    let registry = PluginRegistry::new_for_test();
    let received: Arc<Mutex<Option<serde_json::Value>>> = Arc::new(Mutex::new(None));
    let captured = received.clone();
    registry
        .set_dispatch_for_test("plug-a", move |method, params| {
            assert_eq!(method, "ui/iframeMessage");
            *captured.lock().unwrap() = Some(params.clone());
            json!(null)
        })
        .await;

    let bridge = IframeBridge::new(registry.clone());
    bridge
        .send_message("plug-a", "surface-x", json!({ "hello": "world" }))
        .await
        .unwrap();

    let g = received.lock().unwrap();
    let v = g.as_ref().expect("handler must capture params");
    assert_eq!(v["surface_id"], "surface-x");
    assert_eq!(v["message"], json!({ "hello": "world" }));
}
