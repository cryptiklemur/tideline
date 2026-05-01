mod common;

use std::time::Duration;
use serde_json::json;
use tideline_sdk::Capability::*;

#[tokio::test]
async fn first_crash_restarts_second_crash_sticks() {
    common::ensure_test_plugin_built().await;
    let reg = common::fresh_registry().await;
    for c in [LogWrite, EventsPublish, EventsSubscribe] {
        reg.grant_capability("io.tideline.test", c).await.unwrap();
    }

    reg.start("io.tideline.test").await.unwrap();
    let runtime = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if let Some(rt) = reg.runtime("io.tideline.test").await { return rt; }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }).await.expect("plugin start");
    let transport = runtime.transport().await.expect("transport");

    let _ = transport
        .call(
            "plugin/iframe.message",
            Some(json!({"crash": true})),
            Duration::from_secs(2),
        )
        .await;

    let cleared = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if reg.runtime("io.tideline.test").await.is_none() { return true; }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }).await.unwrap_or(false);
    assert!(cleared, "plugin runtime should clear after first crash");

    let restarted = tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            if reg.runtime("io.tideline.test").await.is_some() { return true; }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }).await.unwrap_or(false);
    assert!(restarted, "plugin did not restart after first crash");

    tokio::time::sleep(Duration::from_millis(200)).await;
    let runtime2 = reg.runtime("io.tideline.test").await.expect("runtime2");
    let transport2 = runtime2.transport().await.expect("transport2");
    let _ = transport2
        .call(
            "plugin/iframe.message",
            Some(json!({"crash": true})),
            Duration::from_secs(2),
        )
        .await;

    let stuck = tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            tokio::time::sleep(Duration::from_millis(200)).await;
            if reg.runtime("io.tideline.test").await.is_none() {
                tokio::time::sleep(Duration::from_secs(1)).await;
                if reg.runtime("io.tideline.test").await.is_none() {
                    return true;
                }
            }
        }
    }).await.unwrap_or(false);
    assert!(stuck, "plugin should stay stopped after second crash in window");
}
