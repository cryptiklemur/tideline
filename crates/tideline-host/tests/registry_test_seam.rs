use serde_json::json;
use tideline_host::registry::PluginRegistry;

#[tokio::test]
async fn new_for_test_returns_empty_contributions() {
    let reg = PluginRegistry::new_for_test();
    let c = reg.contributions().await;
    assert!(c.settings_sections.is_empty());
}

#[tokio::test]
async fn test_handler_responds_to_send_request() {
    let reg = PluginRegistry::new_for_test();
    reg.set_dispatch_for_test("fake.plugin", |method, params| {
        json!({"echoed": method, "params": params})
    })
    .await;
    let resp = reg
        .send_request("fake.plugin", "ping", json!({"x": 1}))
        .await
        .unwrap();
    assert_eq!(resp["echoed"], "ping");
    assert_eq!(resp["params"]["x"], 1);
}

#[tokio::test]
async fn subscribe_receives_set_contributions() {
    use tideline_host::contributions::{Contributions, StatusPillContribution};
    let reg = PluginRegistry::new_for_test();
    let mut rx = reg.subscribe_contributions();
    let mut c = Contributions::default();
    c.status_pills.push(StatusPillContribution {
        plugin_id: "p".into(),
        surface_id: "s".into(),
        label: "ready".into(),
        tone: None,
        icon: None,
        priority: 0,
        tooltip: None,
    });
    reg.set_contributions(c).await;
    let received = rx.recv().await.unwrap();
    assert_eq!(received.status_pills.len(), 1);
    assert_eq!(received.status_pills[0].label, "ready");
}
