mod common;

use std::time::Duration;

#[tokio::test]
async fn plugin_handshakes_and_initialize_succeeds() {
    common::ensure_test_plugin_built().await;
    let reg = common::fresh_registry().await;
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
    let _ = runtime;
    tokio::time::sleep(Duration::from_secs(2)).await;
    reg.stop("io.tideline.test").await;
}
