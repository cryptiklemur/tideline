use std::sync::Arc;
use std::time::Duration;
use async_trait::async_trait;
use serde_json::{json, Value};
use tideline_sdk::{HostClient, Plugin, run};
use tideline_sdk::rpc::{RpcError, error_codes};

const HOST_METHODS: &[&str] = &[
    "host/log.write",
    "host/channel.list",
    "host/channel.get",
    "host/channel.subscribe_meters",
    "host/channel.create",
    "host/channel.update",
    "host/channel.attach_data",
    "host/mix.attach_data",
    "host/levels.read",
    "host/audio.play",
    "host/source.set_mute",
    "host/sources.list",
    "host/audio.position",
    "host/notify",
    "host/ui.iframe.show",
    "host/ui.iframe.hide",
    "host/ui.channel_overlay.focus",
    "host/keybind.register",
    "host/keybind.unregister",
    "host/pipewire.contribute",
    "host/config.namespace.get",
    "host/config.namespace.set",
    "host/config.read",
    "host/config.write",
    "host/fs.read",
    "host/fs.write",
    "host/net.http",
    "host/process.spawn",
    "host/secrets.read",
    "host/secrets.write",
];

struct TestPlugin;

#[async_trait]
impl Plugin for TestPlugin {
    async fn on_ready(&self, host: Arc<HostClient>) {
        let _ = host.initialize("io.tideline.test", "0.0.1", "1.x").await;
        let _ = host.log_write("info", "test plugin ready").await;
        let _ = host.event_subscribe("host:channel.changed").await;
        let _ = host.notify("Test Plugin", "ready").await;
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
            "plugin/smoke_run" => {
                let mut results: Vec<Value> = Vec::with_capacity(HOST_METHODS.len());
                for m in HOST_METHODS {
                    let res = host.call_raw(m, Some(json!({})), Duration::from_secs(2)).await;
                    let entry = match res {
                        Ok(_) => json!({"method": m, "ok": true, "error": Value::Null}),
                        Err(e) => json!({"method": m, "ok": false, "error": e.to_string()}),
                    };
                    results.push(entry);
                }
                Ok(json!({"results": results}))
            }
            "plugin/call_one" => {
                let p = params.unwrap_or(json!({}));
                let method = p.get("method").and_then(|v| v.as_str()).unwrap_or("").to_string();
                let call_params = p.get("params").cloned();
                match host.call_raw(&method, call_params, Duration::from_secs(2)).await {
                    Ok(value) => Ok(json!({"ok": true, "value": value})),
                    Err(tideline_sdk::transport::SdkTransportError::Rpc(rpc_err)) => Ok(json!({
                        "ok": false,
                        "code": rpc_err.code,
                        "message": rpc_err.message,
                    })),
                    Err(other) => Ok(json!({
                        "ok": false,
                        "code": 0,
                        "message": other.to_string(),
                    })),
                }
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
