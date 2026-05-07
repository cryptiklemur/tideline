use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, Runtime};
use tideline_host::PluginRegistry;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiEvent {
    pub surface_id: String,
    pub node_id: String,
    pub value: serde_json::Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context: Option<serde_json::Value>,
}

#[tauri::command]
pub async fn tideline_plugin_contributions<R: Runtime>(
    app: AppHandle<R>,
) -> Result<serde_json::Value, String> {
    let registry = app.state::<Arc<PluginRegistry>>();
    let c = registry.contributions().await;
    serde_json::to_value(c).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn tideline_plugin_emit_event<R: Runtime>(
    app: AppHandle<R>,
    plugin_id: String,
    event: UiEvent,
) -> Result<(), String> {
    let registry = app.state::<Arc<PluginRegistry>>();
    let inner_value = event
        .value
        .get("value")
        .cloned()
        .unwrap_or(serde_json::Value::Null);
    let mut params = serde_json::json!({
        "section_id": event.surface_id,
        "event_id": event.node_id,
        "value": inner_value,
    });
    if let Some(ctx) = event.context {
        params
            .as_object_mut()
            .expect("json! produces object")
            .insert("context".into(), ctx);
    }
    registry
        .send_request(&plugin_id, "settings.section.event", params)
        .await
        .map(|_| ())
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn tideline_plugin_request<R: Runtime>(
    app: AppHandle<R>,
    plugin_id: String,
    method: String,
    params: Option<serde_json::Value>,
) -> Result<serde_json::Value, String> {
    let registry = app.state::<Arc<PluginRegistry>>();
    registry
        .send_request(&plugin_id, &method, params.unwrap_or(serde_json::Value::Null))
        .await
        .map_err(|e| e.to_string())
}


#[tauri::command]
pub async fn tideline_plugin_replay_states<R: Runtime>(app: AppHandle<R>) -> Result<(), String> {
    let registry = app.state::<Arc<PluginRegistry>>();
    registry.replay_plugin_events().await;
    Ok(())
}

#[tauri::command]
pub async fn tideline_plugin_request_permission<R: Runtime>(
    app: AppHandle<R>,
    plugin_id: String,
    capability: String,
    grant: bool,
) -> Result<(), String> {
    let registry = app.state::<Arc<PluginRegistry>>();
    let cap: tideline_sdk::Capability = serde_json::from_value(serde_json::json!(capability))
        .map_err(|e| format!("unknown capability {}: {}", capability, e))?;
    if grant {
        registry
            .grant_capability(&plugin_id, cap)
            .await
            .map_err(|e| e.to_string())?;
    } else {
        registry
            .revoke_capability(&plugin_id, cap)
            .await
            .map_err(|e| e.to_string())?;
    }
    let _ = app.emit(
        "tideline-plugin:permission-resolved",
        serde_json::json!({ "plugin_id": plugin_id, "capability": capability, "grant": grant }),
    );
    Ok(())
}

pub fn spawn_contributions_relay<R: Runtime>(app: &AppHandle<R>, registry: Arc<PluginRegistry>) {
    let mut rx = registry.subscribe_contributions();
    let app_handle = app.clone();
    tauri::async_runtime::spawn(async move {
        while let Ok(snapshot) = rx.recv().await {
            let _ = app_handle.emit("tideline-plugin:contributions", snapshot);
        }
    });
}


pub fn spawn_plugin_events_relay<R: Runtime>(app: &AppHandle<R>, registry: Arc<PluginRegistry>) {
    let mut rx = registry.subscribe_plugin_events();
    let app_handle = app.clone();
    tauri::async_runtime::spawn(async move {
        loop {
            match rx.recv().await {
                Ok(event) => {
                    let _ = app_handle.emit(
                        "tideline-plugin:event",
                        serde_json::json!({ "topic": event.topic, "params": event.params }),
                    );
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                Err(_) => break,
            }
        }
    });
}
