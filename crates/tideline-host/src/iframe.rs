use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::registry::PluginRegistry;

/// Host -> iframe message published on the registry's iframe broadcast channel.
/// The Tauri layer subscribes and forwards each message to the matching webview.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IframeMessage {
    pub plugin_id: String,
    pub surface_id: String,
    pub payload: Value,
}

/// Plugin -> host iframe message body, sent via the `tideline_plugin_iframe_send`
/// Tauri command. The shim stamps `plugin_id` and `surface_id`; the host forwards
/// the message to the owning plugin as the `ui/iframeMessage` JSON-RPC notification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginIframeIncoming {
    pub plugin_id: String,
    pub surface_id: String,
    pub message: Value,
}

/// Pure-Rust bridge between the Tauri webview and the plugin runtime.
/// Holds an `Arc<PluginRegistry>` and forwards messages via JSON-RPC notifications.
#[derive(Clone)]
pub struct IframeBridge {
    registry: Arc<PluginRegistry>,
}

impl IframeBridge {
    pub fn new(registry: Arc<PluginRegistry>) -> Self {
        Self { registry }
    }

    /// Forward an iframe message to the plugin process as a `ui/iframeMessage`
    /// notification. The host stamps `surface_id` so the plugin always knows
    /// which surface the message originated from.
    pub async fn send_message(
        &self,
        plugin_id: &str,
        surface_id: &str,
        message: Value,
    ) -> Result<(), String> {
        let params = serde_json::json!({
            "surface_id": surface_id,
            "message": message,
        });
        self.registry
            .dispatch_event(plugin_id, "ui/iframeMessage", params)
            .await
            .map_err(|e| e.to_string())
    }
}
