use std::sync::Arc;
use async_trait::async_trait;
use serde_json::{json, Value};
use tideline_sdk::{HostClient, Plugin, run};
use tideline_sdk::rpc::{RpcError, error_codes};

struct TestPlugin;

#[async_trait]
impl Plugin for TestPlugin {
    async fn on_ready(&self, host: Arc<HostClient>) {
        let _ = host.initialize("io.tideline.test", "0.0.1", "1.x").await;
        let _ = host.log_write("info", "test plugin ready").await;
        let _ = host.event_subscribe("host:channel.changed").await;
        let _ = host.tray_notify("Test Plugin", "ready").await;
        let _ = host.channel_list().await;
        let _ = host.event_publish(
            "io.tideline.test:smoketest_done",
            json!({"ok": true}),
        ).await;
    }

    async fn on_event(&self, host: Arc<HostClient>, topic: String, params: Value) {
        let _ = host.log_write("info", &format!("event {topic}: {params}")).await;
    }

    async fn on_request(&self, host: Arc<HostClient>, method: String, params: Option<Value>)
        -> Result<Value, RpcError>
    {
        match method.as_str() {
            "plugin/iframe.message" => {
                if let Some(obj) = params.as_ref().and_then(|v| v.as_object()) {
                    if obj.get("crash").and_then(|v| v.as_bool()).unwrap_or(false) {
                        let _ = host.log_write("warn", "crashing on demand").await;
                        std::process::exit(7);
                    }
                }
                Ok(json!({"ack": true}))
            }
            _ => Err(RpcError {
                code: error_codes::METHOD_NOT_FOUND,
                message: format!("unknown method {method}"),
                data: None,
            }),
        }
    }
}

#[tokio::main]
async fn main() {
    run(TestPlugin).await;
}
