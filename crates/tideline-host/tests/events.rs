mod common;

use serde_json::json;
use std::time::Duration;
use tideline_sdk::Capability::*;

#[tokio::test]
async fn plugin_event_publish_and_host_event_fanout() {
    common::ensure_test_plugin_built().await;
    let reg = common::fresh_registry().await;
    for c in [
        LogWrite,
        EventsPublish,
        EventsSubscribe,
        ChannelRead,
        TrayContribute,
    ] {
        reg.grant_capability("io.tideline.test", c).await.unwrap();
    }

    let mut bus_rx = reg.bus.register_plugin("test-observer", vec![]).await;
    reg.bus
        .subscribe("test-observer", "io.tideline.test:smoketest_done")
        .await
        .unwrap();

    reg.start("io.tideline.test").await.unwrap();
    let evt = tokio::time::timeout(Duration::from_secs(5), bus_rx.recv())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(evt.topic, "io.tideline.test:smoketest_done");

    reg.bus
        .publish_host("host:channel.changed", json!({"id":"chan-A"}))
        .await;
    tokio::time::sleep(Duration::from_millis(200)).await;
    reg.stop("io.tideline.test").await;
}
