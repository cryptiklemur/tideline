use std::sync::Arc;
use serde_json::{json, Value};
use tokio::sync::RwLock;
use tideline_sdk::rpc::{RpcError, error_codes};
use crate::capabilities::{CapabilitySet, required_capability_for};
use crate::events::EventBus;

#[derive(Clone)]
pub struct HostContext {
    pub plugin_id: String,
    pub granted: Arc<RwLock<CapabilitySet>>,
    pub bus: EventBus,
}

pub async fn dispatch(ctx: &HostContext, method: &str, params: Option<Value>)
    -> Result<Value, RpcError>
{
    if let Some(required) = required_capability_for(method) {
        if !ctx.granted.read().await.has(required) {
            return Err(RpcError {
                code: error_codes::CAPABILITY_DENIED,
                message: format!("plugin {:?} lacks capability for {method}", ctx.plugin_id),
                data: Some(json!({"required": required})),
            });
        }
    }
    match method {
        "host/initialize" => Ok(json!({
            "host_version": env!("CARGO_PKG_VERSION"),
            "api": "1.x",
        })),
        "host/log.write" => {
            let p = params.unwrap_or(json!({}));
            let level = p.get("level").and_then(|v| v.as_str()).unwrap_or("info").to_string();
            let message = p.get("message").and_then(|v| v.as_str()).unwrap_or("").to_string();
            tracing::info!(plugin = %ctx.plugin_id, level = %level, "{message}");
            Ok(json!({}))
        }
        "host/event.subscribe" => {
            let topic = params.as_ref()
                .and_then(|v| v.get("topic").and_then(|t| t.as_str()))
                .unwrap_or("").to_string();
            ctx.bus.subscribe(&ctx.plugin_id, &topic).await
                .map_err(|e| RpcError { code: error_codes::INTERNAL_ERROR, message: e.to_string(), data: None })?;
            Ok(json!({}))
        }
        "host/event.unsubscribe" => {
            let topic = params.as_ref()
                .and_then(|v| v.get("topic").and_then(|t| t.as_str()))
                .unwrap_or("").to_string();
            ctx.bus.unsubscribe(&ctx.plugin_id, &topic).await
                .map_err(|e| RpcError { code: error_codes::INTERNAL_ERROR, message: e.to_string(), data: None })?;
            Ok(json!({}))
        }
        "host/event.publish" => {
            let p = params.unwrap_or(json!({}));
            let topic = p.get("topic").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let payload = p.get("params").cloned().unwrap_or(json!({}));
            ctx.bus.publish_plugin(&ctx.plugin_id, &topic, payload).await
                .map_err(|e| RpcError {
                    code: error_codes::TOPIC_UNDECLARED,
                    message: e.to_string(),
                    data: None,
                })?;
            Ok(json!({}))
        }
        "host/channel.list" => Ok(json!([])),
        "host/channel.get" => Ok(json!(null)),
        "host/channel.subscribe_meters" => Ok(json!({})),
        "host/channel.create" => Ok(json!({"id": format!("ch-{}", uuid::Uuid::new_v4())})),
        "host/channel.update" => Ok(json!({})),
        "host/channel.attach_data" => Ok(json!({})),
        "host/mix.attach_data" => Ok(json!({})),
        "host/levels.read" => Ok(json!({"channels": []})),
        "host/audio.play" => Ok(json!({})),
        "host/source.set_mute" => Ok(json!({})),
        "host/sources.list" => Ok(json!({"sources": []})),
        "host/audio.position" => Ok(json!({})),
        "host/notify" | "host/notify.send" => Ok(json!({})),
        "host/ui.iframe.show" | "host/ui.iframe.hide" => Ok(json!({})),
        "host/ui.channel_overlay.focus" => Ok(json!({})),
        "host/keybind.register" | "host/keybind.unregister" => Ok(json!({})),
        "host/pipewire.contribute" => Ok(json!({})),
        "host/config.namespace.get" => Ok(json!({})),
        "host/config.namespace.set" => Ok(json!({})),
        "host/config.read" => Ok(json!({})),
        "host/config.write" => Ok(json!({})),
        "host/fs.read" | "host/fs.write" => Ok(json!({})),
        "host/net.http" => Ok(json!({"status": 200, "body": ""})),
        "host/process.spawn" => Ok(json!({"pid": 0})),
        "host/secrets.read" => Ok(json!({"value": null})),
        "host/secrets.write" => Ok(json!({})),
        _ => Err(RpcError {
            code: error_codes::METHOD_NOT_FOUND,
            message: format!("unknown host method {method}"),
            data: None,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tideline_sdk::Capability::*;

    #[tokio::test]
    async fn capability_denied_when_not_granted() {
        let ctx = HostContext {
            plugin_id: "p".into(),
            granted: Arc::new(RwLock::new(CapabilitySet::default())),
            bus: EventBus::new(),
        };
        let err = dispatch(&ctx, "host/log.write", Some(json!({}))).await.unwrap_err();
        assert_eq!(err.code, error_codes::CAPABILITY_DENIED);
    }

    #[tokio::test]
    async fn capability_granted_passes() {
        let ctx = HostContext {
            plugin_id: "p".into(),
            granted: Arc::new(RwLock::new(CapabilitySet::new([LogWrite]))),
            bus: EventBus::new(),
        };
        dispatch(&ctx, "host/log.write", Some(json!({"level":"info","message":"ok"})))
            .await.unwrap();
    }

    #[tokio::test]
    async fn initialize_does_not_require_capability() {
        let ctx = HostContext {
            plugin_id: "p".into(),
            granted: Arc::new(RwLock::new(CapabilitySet::default())),
            bus: EventBus::new(),
        };
        dispatch(&ctx, "host/initialize", None).await.unwrap();
    }
}
