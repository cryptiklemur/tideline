use serde_json::json;
use tideline_host::registry::{ContribKind, PluginRegistry};

#[tokio::test]
async fn new_for_test_returns_empty_contributions() {
    let reg = PluginRegistry::new_for_test();
    let c = reg.contributions().await;
    assert!(c.settings_sections.is_empty());
}

#[tokio::test]
async fn test_handler_responds_to_send_request() {
    let reg = PluginRegistry::new_for_test();
    reg.set_dispatch_for_test(
        "fake.plugin",
        |method, params| json!({"echoed": method, "params": params}),
    )
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

#[tokio::test]
async fn register_status_pill_stamps_plugin_id_and_broadcasts() {
    let reg = PluginRegistry::new_for_test();
    let mut rx = reg.subscribe_contributions();
    reg.register_contribution(
        "io.test.a",
        ContribKind::StatusPill,
        json!({
            "plugin_id": "lying.plugin",
            "surface_id": "main",
            "label": "Hello"
        }),
    )
    .await
    .unwrap();
    let received = rx.recv().await.unwrap();
    assert_eq!(received.status_pills.len(), 1);
    assert_eq!(received.status_pills[0].plugin_id, "io.test.a");
    assert_eq!(received.status_pills[0].surface_id, "main");
    assert_eq!(received.status_pills[0].label, "Hello");
    let snap = reg.contributions().await;
    assert_eq!(snap.status_pills.len(), 1);
}

#[tokio::test]
async fn register_multi_plugin_merges_into_global() {
    let reg = PluginRegistry::new_for_test();
    reg.register_contribution(
        "io.test.a",
        ContribKind::StatusPill,
        json!({"surface_id": "a1", "label": "A"}),
    )
    .await
    .unwrap();
    reg.register_contribution(
        "io.test.b",
        ContribKind::StatusPill,
        json!({"surface_id": "b1", "label": "B"}),
    )
    .await
    .unwrap();
    let snap = reg.contributions().await;
    assert_eq!(snap.status_pills.len(), 2);
    let ids: Vec<&str> = snap
        .status_pills
        .iter()
        .map(|p| p.plugin_id.as_str())
        .collect();
    assert!(ids.contains(&"io.test.a"));
    assert!(ids.contains(&"io.test.b"));
}

#[tokio::test]
async fn register_replace_on_same_surface_id() {
    let reg = PluginRegistry::new_for_test();
    reg.register_contribution(
        "io.test.a",
        ContribKind::SettingsSection,
        json!({
            "surface_id": "main",
            "title": "First",
            "tree": {}
        }),
    )
    .await
    .unwrap();
    reg.register_contribution(
        "io.test.a",
        ContribKind::SettingsSection,
        json!({
            "surface_id": "main",
            "title": "Second",
            "tree": {}
        }),
    )
    .await
    .unwrap();
    let snap = reg.contributions().await;
    assert_eq!(snap.settings_sections.len(), 1);
    assert_eq!(snap.settings_sections[0].title, "Second");
}

#[tokio::test]
async fn unregister_removes_only_matching_surface() {
    let reg = PluginRegistry::new_for_test();
    reg.register_contribution(
        "io.test.a",
        ContribKind::StatusPill,
        json!({"surface_id": "p1", "label": "P1"}),
    )
    .await
    .unwrap();
    reg.register_contribution(
        "io.test.a",
        ContribKind::StatusPill,
        json!({"surface_id": "p2", "label": "P2"}),
    )
    .await
    .unwrap();
    reg.unregister_contribution("io.test.a", ContribKind::StatusPill, "p1")
        .await
        .unwrap();
    let snap = reg.contributions().await;
    assert_eq!(snap.status_pills.len(), 1);
    assert_eq!(snap.status_pills[0].surface_id, "p2");
}

#[tokio::test]
async fn unregister_unknown_id_is_a_noop() {
    let reg = PluginRegistry::new_for_test();
    reg.register_contribution(
        "io.test.a",
        ContribKind::StatusPill,
        json!({"surface_id": "p1", "label": "P1"}),
    )
    .await
    .unwrap();
    reg.unregister_contribution("io.test.a", ContribKind::StatusPill, "ghost")
        .await
        .unwrap();
    let snap = reg.contributions().await;
    assert_eq!(snap.status_pills.len(), 1);
}

#[tokio::test]
async fn evict_plugin_contributions_drops_only_that_plugin() {
    let reg = PluginRegistry::new_for_test();
    reg.register_contribution(
        "io.test.a",
        ContribKind::StatusPill,
        json!({"surface_id": "a1", "label": "A"}),
    )
    .await
    .unwrap();
    reg.register_contribution(
        "io.test.b",
        ContribKind::StatusPill,
        json!({"surface_id": "b1", "label": "B"}),
    )
    .await
    .unwrap();
    reg.evict_plugin_contributions("io.test.a").await;
    let snap = reg.contributions().await;
    assert_eq!(snap.status_pills.len(), 1);
    assert_eq!(snap.status_pills[0].plugin_id, "io.test.b");
}

#[tokio::test]
async fn stop_evicts_contributions_for_uninstalled_plugin() {
    let reg = PluginRegistry::new_for_test();
    reg.register_contribution(
        "io.test.a",
        ContribKind::StatusPill,
        json!({"surface_id": "a1", "label": "A"}),
    )
    .await
    .unwrap();
    assert_eq!(reg.contributions().await.status_pills.len(), 1);
    reg.stop("io.test.a").await;
    assert_eq!(reg.contributions().await.status_pills.len(), 0);
}

#[tokio::test]
async fn register_each_kind_lands_in_correct_slot() {
    let reg = PluginRegistry::new_for_test();
    reg.register_contribution(
        "p",
        ContribKind::SettingsSection,
        json!({"surface_id": "s1", "title": "T", "tree": {}}),
    )
    .await
    .unwrap();
    reg.register_contribution(
        "p",
        ContribKind::StatusPill,
        json!({"surface_id": "p1", "label": "L"}),
    )
    .await
    .unwrap();
    reg.register_contribution(
        "p",
        ContribKind::ChannelOverlay,
        json!({
            "surface_id": "o1",
            "placement": "detail",
            "channel_filter": {"kind": "all"},
            "tree": {}
        }),
    )
    .await
    .unwrap();
    reg.register_contribution(
        "p",
        ContribKind::IframeSurface,
        json!({"surface_id": "f1", "entry_path": "ui/index.html"}),
    )
    .await
    .unwrap();
    reg.register_contribution(
        "p",
        ContribKind::TrayItem,
        json!({"item_id": "t1", "label": "Quit"}),
    )
    .await
    .unwrap();
    reg.register_contribution(
        "p",
        ContribKind::KeybindAction,
        json!({"action_id": "a1", "label": "Toggle"}),
    )
    .await
    .unwrap();
    let snap = reg.contributions().await;
    assert_eq!(snap.settings_sections.len(), 1);
    assert_eq!(snap.status_pills.len(), 1);
    assert_eq!(snap.channel_overlays.len(), 1);
    assert_eq!(snap.iframe_surfaces.len(), 1);
    assert_eq!(snap.tray_items.len(), 1);
    assert_eq!(snap.keybind_actions.len(), 1);
}

#[tokio::test]
async fn register_invalid_payload_returns_error() {
    let reg = PluginRegistry::new_for_test();
    let err = reg
        .register_contribution("p", ContribKind::StatusPill, json!({"surface_id": "s"}))
        .await
        .unwrap_err();
    let s = err.to_string();
    assert!(s.contains("invalid contribution"), "got: {s}");
}
