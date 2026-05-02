mod binding;
mod config;
mod evdev_listener;
mod mute;
mod portal_listener;
mod runtime;
mod state;
mod ui;

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use serde_json::{json, Value};
use tracing::{error, info, warn};

use tideline_sdk::rpc::{error_codes, RpcError};
use tideline_sdk::{run, HostClient, Plugin};

use crate::binding::Binding;
use crate::config::PluginConfig;
use crate::runtime::{CaptureMethod, PttRuntime};

const PLUGIN_ID: &str = "tideline-ptt";
const VERSION: &str = "0.1.0";
const SDK_VERSION: &str = "1.0";
const SECTION_ID: &str = "tideline-ptt";

struct PttPlugin {
    runtime: Arc<PttRuntime>,
}

#[async_trait]
impl Plugin for PttPlugin {
    async fn on_ready(&self, host: Arc<HostClient>) {
        self.runtime.set_host(host.clone());

        if let Err(e) = host.initialize(PLUGIN_ID, VERSION, SDK_VERSION).await {
            error!(?e, "initialize failed");
            return;
        }

        match host.config_namespace_get(PLUGIN_ID).await {
            Ok(v) => {
                let cfg = PluginConfig::from_value(&v);
                *self.runtime.config.lock().await = cfg.clone();
                let mut state = self.runtime.state.lock().await;
                state.mode = cfg.mode;
                state.hold_active = false;
            }
            Err(e) => warn!(?e, "config_namespace_get failed; using defaults"),
        }

        let runtime = self.runtime.clone();
        tokio::spawn(async move {
            let portal_result = tokio::time::timeout(
                Duration::from_secs(3),
                portal_listener::try_start(runtime.clone()),
            )
            .await;
            match portal_result {
                Ok(Ok(())) => {
                    runtime.set_capture_method(CaptureMethod::Portal).await;
                    info!("ptt capture: portal");
                }
                Ok(Err(e)) => {
                    warn!(error = %e, "portal unavailable, falling back to evdev");
                    evdev_listener::start(runtime.clone()).await;
                    runtime.set_capture_method(CaptureMethod::Evdev).await;
                }
                Err(_) => {
                    warn!("portal start timed out, falling back to evdev");
                    evdev_listener::start(runtime.clone()).await;
                    runtime.set_capture_method(CaptureMethod::Evdev).await;
                }
            }
        });

        info!(plugin = PLUGIN_ID, "ready");
    }

    async fn on_notification(&self, _host: Arc<HostClient>, method: String, params: Option<Value>) {
        if method != "keybind/invoke" {
            return;
        }
        let p = params.unwrap_or(Value::Null);
        let action_id = p.get("action_id").and_then(|v| v.as_str()).unwrap_or("");
        match action_id {
            "toggle_mode" => self.runtime.toggle_mode().await,
            "hold_press" => self.runtime.hold_press().await,
            "hold_release" => self.runtime.hold_release().await,
            other => warn!(action_id = other, "unknown keybind action"),
        }
    }

    async fn on_request(
        &self,
        host: Arc<HostClient>,
        method: String,
        params: Option<Value>,
    ) -> Result<Value, RpcError> {
        match method.as_str() {
            "settings.section.render" => self.handle_render(params).await,
            "settings.section.event" => self.handle_event(host, params).await,
            other => Err(RpcError {
                code: error_codes::METHOD_NOT_FOUND,
                message: format!("unknown method {other}"),
                data: None,
            }),
        }
    }
}

impl PttPlugin {
    async fn handle_render(&self, params: Option<Value>) -> Result<Value, RpcError> {
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
        let cfg = self.runtime.config.lock().await.clone();
        let cm = *self.runtime.capture_method.lock().await;
        let err = self.runtime.error.lock().await.clone();
        Ok(ui::settings_section(&cfg, cm, err.as_deref()))
    }

    async fn handle_event(
        &self,
        host: Arc<HostClient>,
        params: Option<Value>,
    ) -> Result<Value, RpcError> {
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

        match event_id.as_str() {
            "tideline-ptt:toggle_mode" => {
                self.runtime.toggle_mode().await;
            }
            "tideline-ptt:set_input_device" => {
                if let Some(node) = value.as_str() {
                    let mut cfg = self.runtime.config.lock().await;
                    cfg.input_device = Some(node.to_string());
                    let snap = cfg.clone();
                    drop(cfg);
                    if let Err(e) = config::save(&host, PLUGIN_ID, &snap).await {
                        error!(?e, "config save failed");
                    }
                }
            }
            "tideline-ptt:set_mode_toggle_binding" | "tideline-ptt:set_hold_binding" => {
                if let Some(s) = value.as_str() {
                    match Binding::parse(s) {
                        Ok(b) => {
                            let mut cfg = self.runtime.config.lock().await;
                            if event_id.ends_with("mode_toggle_binding") {
                                cfg.mode_toggle_binding = Some(b);
                            } else {
                                cfg.hold_binding = Some(b);
                            }
                            let snap = cfg.clone();
                            drop(cfg);
                            if let Err(e) = config::save(&host, PLUGIN_ID, &snap).await {
                                error!(?e, "config save failed");
                            }
                        }
                        Err(err) => warn!(input = s, error = %err, "failed to parse binding"),
                    }
                }
            }
            "tideline-ptt:list_sources" => {
                let v = host
                    .call_raw("host/sources.list", Some(json!({})), Duration::from_secs(5))
                    .await
                    .unwrap_or(json!([]));
                return Ok(v);
            }
            "tideline-ptt:install_udev_rule" => {
                if let Err(e) = install_udev_rule().await {
                    error!(?e, "install_udev_rule failed");
                    return Err(RpcError {
                        code: error_codes::INTERNAL_ERROR,
                        message: format!("install_udev_rule: {e}"),
                        data: None,
                    });
                }
            }
            "tideline-ptt:configure_shortcuts" => {
                let _ = tokio::process::Command::new("kcmshell6")
                    .arg("kcm_keys")
                    .spawn();
            }
            other => warn!(event_id = other, "unknown settings event"),
        }
        Ok(json!({}))
    }
}

async fn install_udev_rule() -> Result<(), String> {
    let rule = r#"KERNEL=="event*", SUBSYSTEM=="input", MODE="0660", GROUP="input""#;
    let path = "/etc/udev/rules.d/99-tideline-input.rules";
    let script = format!(
        "echo '{}' | tee {} && udevadm control --reload-rules && udevadm trigger",
        rule, path
    );
    let status = tokio::process::Command::new("pkexec")
        .arg("sh")
        .arg("-c")
        .arg(&script)
        .status()
        .await
        .map_err(|e| format!("pkexec spawn: {e}"))?;
    if !status.success() {
        return Err(format!("pkexec exited with status {status}"));
    }
    Ok(())
}

#[tokio::main(flavor = "multi_thread", worker_threads = 4)]
async fn main() {
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let runtime = PttRuntime::new_without_client(PLUGIN_ID, PluginConfig::default());
    let plugin = PttPlugin { runtime };
    run(plugin).await;
}
