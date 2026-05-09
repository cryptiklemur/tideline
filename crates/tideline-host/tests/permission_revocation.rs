mod common;

use serde_json::{json, Value};
use std::time::Duration;
use tideline_sdk::Capability;

#[tokio::test]
async fn revocation_notifies_plugin_via_permissions_changed() {
    common::ensure_test_plugin_built().await;
    let reg = common::fresh_registry().await;
    for c in [
        Capability::LogWrite,
        Capability::EventsPublish,
        Capability::EventsSubscribe,
    ] {
        reg.grant_capability("io.tideline.test", c).await.unwrap();
    }
    reg.start("io.tideline.test").await.unwrap();

    let runtime = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if let Some(rt) = reg.runtime("io.tideline.test").await {
                return rt;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .expect("plugin start");

    tokio::time::sleep(Duration::from_millis(200)).await;

    let removed = reg
        .revoke_capability("io.tideline.test", Capability::EventsPublish)
        .await
        .unwrap();
    assert!(
        removed,
        "EventsPublish should have been granted before revoke"
    );

    let transport = runtime.transport().await.expect("transport");
    let observed: Value = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let resp = transport
                .call(
                    "plugin/last_perms_change",
                    Some(json!({})),
                    Duration::from_secs(2),
                )
                .await
                .expect("call");
            if !resp.is_null() {
                return resp;
            }
            tokio::time::sleep(Duration::from_millis(75)).await;
        }
    })
    .await
    .expect("plugin should observe permissions.changed");

    let granted = observed
        .get("granted")
        .and_then(|v| v.as_array())
        .expect("granted array")
        .iter()
        .filter_map(|v| v.as_str().map(|s| s.to_string()))
        .collect::<Vec<_>>();
    assert!(
        !granted.iter().any(|c| c == "events.publish"),
        "events.publish should be revoked, granted={granted:?}"
    );
    assert!(
        granted.iter().any(|c| c == "log.write"),
        "log.write should still be granted, granted={granted:?}"
    );

    reg.stop("io.tideline.test").await;
}
