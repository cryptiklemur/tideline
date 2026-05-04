pub mod config;
pub mod device;
pub mod runtime;
pub mod settings_ui;

use std::sync::Arc;

use async_trait::async_trait;
use serde_json::{json, Value};
use tracing::{error, info, warn};

use tideline_sdk::rpc::{error_codes, RpcError};
use tideline_sdk::{run, HostClient, Plugin};

use crate::config::PluginConfig;
use crate::device::{HidLedWriter, LedWriter};
use crate::runtime::Runtime;
use crate::settings_ui::{render, EVT_LED_ENABLED, SECTION_ID};

const PLUGIN_ID: &str = "tideline-wave-xlr";
const VERSION: &str = "0.1.0";
const SDK_VERSION: &str = "1.0";
const TRANSMIT_TOPIC: &str = "tideline-ptt:transmit_changed";

struct WaveXlrPlugin {
    runtime: Arc<Runtime>,
    present: bool,
}

#[async_trait]
impl Plugin for WaveXlrPlugin {
    async fn on_ready(&self, host: Arc<HostClient>) {
        if let Err(e) = host.initialize(PLUGIN_ID, VERSION, SDK_VERSION).await {
            error!(?e, "initialize failed");
            return;
        }
        if self.present {
            if let Err(e) = host
                .register_settings_section(json!({
                    "surface_id": SECTION_ID,
                    "title": "Wave XLR",
                    "icon": { "name": "microphone-stand" },
                    "priority": 200,
                    "tree": {},
                }))
                .await
            {
                warn!(?e, "register_settings_section failed");
            }
        }
        match host.config_namespace_get(PLUGIN_ID).await {
            Ok(v) => self.runtime.set_config(PluginConfig::from_value(&v)),
            Err(e) => warn!(?e, "config_namespace_get failed; using defaults"),
        }
        if let Err(e) = host.event_subscribe(TRANSMIT_TOPIC).await {
            error!(?e, "event_subscribe failed");
        }
        info!(plugin = PLUGIN_ID, present = self.present, "ready");
    }

    async fn on_event(&self, _host: Arc<HostClient>, topic: String, params: Value) {
        if topic != TRANSMIT_TOPIC {
            return;
        }
        let transmitting = params
            .get("transmitting")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        self.runtime.on_transmit_changed(transmitting);
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
                let cfg = self.runtime.config_snapshot();
                Ok(serde_json::to_value(render(&cfg, self.present)).unwrap_or(Value::Null))
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

                if event_id != EVT_LED_ENABLED {
                    warn!(?event_id, "unknown settings event");
                    return Ok(json!({}));
                }

                if let Some(updated_cfg) = self.runtime.on_settings_event(&event_id, value) {
                    if let Ok(v) = serde_json::to_value(&updated_cfg) {
                        if let Err(e) = host.config_namespace_set(PLUGIN_ID, v).await {
                            error!(?e, "config_namespace_set failed");
                        }
                    }
                    let tree = serde_json::to_value(render(&updated_cfg, self.present))
                        .unwrap_or(Value::Null);
                    if let Err(e) = host.settings_section_render(SECTION_ID, tree).await {
                        error!(?e, "settings_section_render failed");
                    }
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
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let present = device::is_present();
    let writer: Box<dyn LedWriter> = Box::new(HidLedWriter);
    let runtime = Arc::new(Runtime::new(PluginConfig::default(), writer, present));

    let plugin = WaveXlrPlugin {
        runtime,
        present,
    };
    run(plugin).await;
}
