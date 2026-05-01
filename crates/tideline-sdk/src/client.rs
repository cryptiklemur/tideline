use std::sync::Arc;
use std::time::Duration;
use serde_json::{json, Value};
use crate::transport::{SdkTransportError, StdioTransport};

pub struct HostClient {
    transport: Arc<StdioTransport>,
}

impl HostClient {
    pub fn new(transport: Arc<StdioTransport>) -> Self { Self { transport } }

    pub async fn initialize(&self, plugin_id: &str, version: &str, sdk_version: &str)
        -> Result<Value, SdkTransportError>
    {
        self.transport.call(
            "host/initialize",
            Some(json!({"plugin_id": plugin_id, "version": version, "sdk_version": sdk_version})),
            Duration::from_secs(5),
        ).await
    }

    pub async fn log_write(&self, level: &str, message: &str) -> Result<(), SdkTransportError> {
        self.transport.call(
            "host/log.write",
            Some(json!({"level": level, "message": message})),
            Duration::from_secs(2),
        ).await.map(|_| ())
    }

    pub async fn event_subscribe(&self, topic: &str) -> Result<(), SdkTransportError> {
        self.transport.call(
            "host/event.subscribe",
            Some(json!({"topic": topic})),
            Duration::from_secs(2),
        ).await.map(|_| ())
    }

    pub async fn event_publish(&self, topic: &str, params: Value) -> Result<(), SdkTransportError> {
        self.transport.call(
            "host/event.publish",
            Some(json!({"topic": topic, "params": params})),
            Duration::from_secs(2),
        ).await.map(|_| ())
    }

    pub async fn channel_list(&self) -> Result<Value, SdkTransportError> {
        self.transport.call("host/channel.list", Some(json!({})), Duration::from_secs(2)).await
    }

    pub async fn notify(&self, title: &str, body: &str) -> Result<(), SdkTransportError> {
        self.transport.call(
            "host/notify",
            Some(json!({"title": title, "body": body})),
            Duration::from_secs(2),
        ).await.map(|_| ())
    }
}
