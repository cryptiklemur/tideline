mod common;

use std::time::Duration;
use serde_json::json;
use tideline_sdk::Capability;
use tideline_sdk::rpc::error_codes;

#[tokio::test]
async fn host_rejects_uncapable_call_with_minus_32001() {
    common::ensure_test_plugin_built().await;
    let reg = common::fresh_registry().await;

    reg.grant_capability("io.tideline.test", Capability::FsRead).await.unwrap();
    reg.start("io.tideline.test").await.unwrap();

    let runtime = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if let Some(rt) = reg.runtime("io.tideline.test").await { return rt; }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }).await.expect("plugin start");

    let removed = reg.revoke_capability("io.tideline.test", Capability::FsRead).await.unwrap();
    assert!(removed, "FsRead should have been granted before revoke");
    tokio::time::sleep(Duration::from_millis(150)).await;

    let transport = runtime.transport().await.expect("transport");
    let resp = transport
        .call(
            "plugin/call_one",
            Some(json!({"method": "host/fs.read", "params": {"path": "/tmp/a"}})),
            Duration::from_secs(5),
        )
        .await
        .expect("call_one");

    let ok = resp.get("ok").and_then(|v| v.as_bool()).unwrap_or(true);
    assert!(!ok, "expected denial, got: {resp}");
    let code = resp.get("code").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
    assert_eq!(code, error_codes::CAPABILITY_DENIED,
        "expected -32001, got code={code} resp={resp}");

    reg.stop("io.tideline.test").await;
}
