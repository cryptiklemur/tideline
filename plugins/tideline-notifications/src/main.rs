use std::sync::Arc;

use async_trait::async_trait;
use serde_json::{json, Value};
use tokio::sync::Mutex;
use tracing::{error, info, warn};

use tideline_notifications::config::NotifConfig;
use tideline_notifications::settings::{render, EVT_ENABLED, SECTION_ID};
use tideline_sdk::rpc::{error_codes, RpcError};
use tideline_sdk::{run, HostClient, Plugin};

const TOPIC_PTT_MODE: &str = "tideline-ptt:mode_changed";
const PLUGIN_ID: &str = "tideline-notifications";
const SDK_VERSION: &str = "1.0";
const VERSION: &str = "1.0.0";

struct NotifPlugin {
    cfg: Arc<Mutex<NotifConfig>>,
}

#[async_trait]
impl Plugin for NotifPlugin {
    async fn on_ready(&self, host: Arc<HostClient>) {
        if let Err(e) = host.initialize(PLUGIN_ID, VERSION, SDK_VERSION).await {
            error!(?e, "initialize failed");
            return;
        }
        if let Err(e) = host
            .register_settings_section(json!({
                "surface_id": SECTION_ID,
                "title": "PTT Notifications",
                "icon": { "name": "bell" },
                "priority": 120,
                "parent_surface_id": "tideline-ptt",
                "tree": {},
            }))
            .await
        {
            warn!(?e, "register_settings_section failed");
        }
        match host.config_namespace_get(PLUGIN_ID).await {
            Ok(v) => *self.cfg.lock().await = NotifConfig::from_json(&v),
            Err(e) => warn!(?e, "config_namespace_get failed; using defaults"),
        }
        if let Err(e) = host.event_subscribe(TOPIC_PTT_MODE).await {
            error!(?e, "event_subscribe failed");
        }
        let cfg_now = self.cfg.lock().await.clone();
        if let Err(e) = host
            .settings_section_render(SECTION_ID, render(&cfg_now))
            .await
        {
            warn!(?e, "initial settings_section_render failed");
        }
        info!(plugin = PLUGIN_ID, "ready");
    }

    async fn on_event(&self, host: Arc<HostClient>, topic: String, params: Value) {
        if topic != TOPIC_PTT_MODE {
            return;
        }
        let cfg_now = self.cfg.lock().await.clone();
        if !cfg_now.enabled {
            return;
        }
        let mode = params.get("mode").and_then(|m| m.as_str()).unwrap_or("");
        let source_name = params
            .get("source_name")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let (mode_label, body) = match mode {
            "open" => ("Open mic", "Microphone always on"),
            "ptt" => ("PTT mode", "Hold the bind to transmit"),
            other => {
                warn!(?other, "unknown mode in mode_changed");
                return;
            }
        };
        let title = if let Some(src) = source_name {
            let label = match host.list_input_sources().await {
                Ok(sources) => sources
                    .iter()
                    .find(|s| s.name == src)
                    .map(|s| s.description.clone())
                    .unwrap_or_else(|| src.clone()),
                Err(_) => src.clone(),
            };
            format!("{} — {}", label, mode_label)
        } else {
            mode_label.to_string()
        };
        if let Err(e) = host.notify(&title, body).await {
            error!(?e, "host/notify failed");
        }
    }

    async fn on_request(
        &self,
        host: Arc<HostClient>,
        method: String,
        params: Option<Value>,
    ) -> Result<Value, RpcError> {
        match method.as_str() {
            "settings.section.render" => {
                let section_id = params
                    .as_ref()
                    .and_then(|v| v.get("section_id"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                if section_id != SECTION_ID {
                    return Err(RpcError {
                        code: error_codes::INVALID_PARAMS,
                        message: format!("unknown section_id {section_id}"),
                        data: None,
                    });
                }
                let cfg_now = self.cfg.lock().await.clone();
                Ok(serde_json::to_value(render(&cfg_now)).unwrap_or(Value::Null))
            }
            "settings.section.event" => {
                let p = params.unwrap_or(Value::Null);
                let section_id = p.get("section_id").and_then(|v| v.as_str()).unwrap_or("");
                if section_id != SECTION_ID {
                    return Err(RpcError {
                        code: error_codes::INVALID_PARAMS,
                        message: format!("unknown section_id {section_id}"),
                        data: None,
                    });
                }
                let event_id = p
                    .get("event_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let value = p.get("value").cloned().unwrap_or(Value::Null);
                let mut cfg_lock = self.cfg.lock().await;
                if event_id == EVT_ENABLED {
                    if let Some(b) = value.as_bool() {
                        cfg_lock.enabled = b;
                    }
                } else {
                    warn!(?event_id, "unknown settings event");
                }
                let cfg_clone = cfg_lock.clone();
                drop(cfg_lock);
                if let Ok(v) = serde_json::to_value(&cfg_clone) {
                    if let Err(e) = host.config_namespace_set(PLUGIN_ID, v).await {
                        error!(?e, "config_namespace_set failed");
                    }
                }
                if let Err(e) = host
                    .settings_section_render(SECTION_ID, render(&cfg_clone))
                    .await
                {
                    error!(?e, "settings_section_render failed");
                }
                Ok(json!({}))
            }
            _ => Err(RpcError {
                code: error_codes::METHOD_NOT_FOUND,
                message: format!("unknown method {method}"),
                data: None,
            }),
        }
    }
}

#[tokio::main(flavor = "multi_thread", worker_threads = 2)]
async fn main() {
    let _log_guard = tideline_sdk::logging::init("tideline-notifications");
    let plugin = NotifPlugin {
        cfg: Arc::new(Mutex::new(NotifConfig::default())),
    };
    run(plugin).await;
}
