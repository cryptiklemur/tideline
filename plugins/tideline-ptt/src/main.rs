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
const INPUT_OVERLAY_ID: &str = "tideline-ptt-input";

struct PttPlugin {
    runtime: Arc<PttRuntime>,
}

#[async_trait]
impl Plugin for PttPlugin {
    async fn on_ready(&self, host: Arc<HostClient>) {
        eprintln!("PTT on_ready called!");
        self.runtime.set_host(host.clone());

        if let Err(e) = host.initialize(PLUGIN_ID, VERSION, SDK_VERSION).await {
            error!(?e, "initialize failed");
            return;
        }

        if let Err(e) = host
            .register_settings_section(json!({
                "surface_id": SECTION_ID,
                "title": "Push to Talk",
                "icon": { "name": "mic" },
                "priority": 100,
                "tree": {},
            }))
            .await
        {
            warn!(?e, "register_settings_section failed");
        }

if let Err(e) = push_input_overlay(&host, &PluginConfig::default()).await {
            warn!(?e, "register_input_overlay failed");
        }

        match host.config_namespace_get(PLUGIN_ID).await {
            Ok(v) => {
                eprintln!("PTT: config loaded raw={}", v);
                let cfg = PluginConfig::from_value(&v);
                eprintln!("PTT: config parsed enabled_sources={:?} mode_by_source={:?}", cfg.enabled_sources, cfg.mode_by_source);
                *self.runtime.config.lock().await = cfg.clone();
                let mut state_map = self.runtime.state_by_source.lock().await;
                state_map.clear();
                for src in &cfg.enabled_sources {
                    let mode = cfg.mode_for(src);
                    eprintln!("PTT: init state source={} mode={:?}", src, mode);
                    state_map.insert(
                        src.clone(),
                        crate::state::PerSourceState {
                            mode,
                            hold_active: false,
                        },
                    );
                }
            }
            Err(e) => {
                eprintln!("PTT: config_namespace_get FAILED err={:?}", e);
                warn!(?e, "config_namespace_get failed; using defaults");
            }
        }

        let runtime = self.runtime.clone();
        let host_for_tree = host.clone();
        eprintln!("PTT: spawning tree render task");
        tokio::spawn(async move {
            eprintln!("PTT: tree render task started");
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
            eprintln!("PTT: about to call settings_section_render");
            let cm = *runtime.capture_method.lock().await;
            let cfg = runtime.config.lock().await.clone();
            let err = runtime.error.lock().await.clone();
            let sources = host_for_tree.list_input_sources().await.unwrap_or_default();
            if let Err(e) = host_for_tree.settings_section_render(
                SECTION_ID,
                ui::settings_section(&cfg, cm, err.as_deref(), &sources),
            ).await {
                warn!(?e, "initial settings_section_render failed");
            }
            if let Err(e) = push_input_overlay(&host_for_tree, &cfg).await {
                warn!(?e, "initial input overlay re-register failed");
            }
            runtime.publish_all_states().await;
            eprintln!("PTT: settings_section_render completed");
        });

        if let Err(e) = host.event_subscribe("host:pipewire_restarted").await {
            warn!(?e, "event_subscribe(host:pipewire_restarted) failed");
        }

        info!(plugin = PLUGIN_ID, "ready");
    }

    async fn on_event(&self, _host: Arc<HostClient>, topic: String, _params: Value) {
        if topic == "host:pipewire_restarted" {
            tracing::info!("PTT: pipewire restarted, reapplying mutes");
            self.runtime.reapply_all_mutes().await;
        }
    }

    async fn on_notification(&self, _host: Arc<HostClient>, method: String, params: Option<Value>) {
        if method != "keybind/invoke" {
            return;
        }
        let p = params.unwrap_or(Value::Null);
        let action_id = p.get("action_id").and_then(|v| v.as_str()).unwrap_or("");
        let (op, source) = match action_id.split_once(':') {
            Some((op, src)) => (op, Some(src.to_string())),
            None => (action_id, None),
        };
        let sources: Vec<String> = match source {
            Some(s) => vec![s],
            None => self.runtime.config.lock().await.enabled_sources.clone(),
        };
        for src in sources {
            match op {
                "toggle_mode" => self.runtime.toggle_mode(&src).await,
                "hold_press" => self.runtime.hold_press(&src).await,
                "hold_release" => self.runtime.hold_release(&src).await,
                other => {
                    warn!(action_id = other, "unknown keybind action");
                    break;
                }
            }
        }
    }

    async fn on_request(
        &self,
        host: Arc<HostClient>,
        method: String,
        params: Option<Value>,
    ) -> Result<Value, RpcError> {
        match method.as_str() {
            "settings.section.render" => self.handle_render(host, params).await,
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
    async fn handle_render(&self, host: Arc<HostClient>, params: Option<Value>) -> Result<Value, RpcError> {
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
        let sources = host.list_input_sources().await.unwrap_or_default();
        Ok(ui::settings_section(&cfg, cm, err.as_deref(), &sources))
    }

    async fn handle_event(
        &self,
        host: Arc<HostClient>,
        params: Option<Value>,
    ) -> Result<Value, RpcError> {
        let p = params.unwrap_or(Value::Null);
        let section_id = p.get("section_id").and_then(|v| v.as_str()).unwrap_or("");
        if section_id != SECTION_ID && section_id != INPUT_OVERLAY_ID {
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
        let context_source = p
            .get("context")
            .and_then(|c| c.get("source_name"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        match event_id.as_str() {
            "tideline-ptt:toggle_mode" => {
                let sources: Vec<String> = match context_source.clone() {
                    Some(s) => vec![s],
                    None => self.runtime.config.lock().await.enabled_sources.clone(),
                };
                for src in sources {
                    self.runtime.toggle_mode(&src).await;
                }
            }
            "tideline-ptt:input_enabled" => {
                let Some(source_name) = context_source.clone() else {
                    warn!("input_enabled event missing context.source_name");
                    return Ok(json!({}));
                };
                let enabled = value.as_bool().unwrap_or(false);
                let mut cfg = self.runtime.config.lock().await;
                let already = cfg.enabled_sources.iter().any(|s| s == &source_name);
                if enabled && !already {
                    cfg.enabled_sources.push(source_name.clone());
                } else if !enabled {
                    cfg.enabled_sources.retain(|s| s != &source_name);
                    cfg.mode_by_source.remove(&source_name);
                }
                let snap = cfg.clone();
                drop(cfg);
                if let Err(e) = config::save(&host, PLUGIN_ID, &snap).await {
                    error!(?e, "config save failed");
                }
                if !enabled {
                    self.runtime
                        .state_by_source
                        .lock()
                        .await
                        .remove(&source_name);
                }
                if let Err(e) = push_input_overlay(&host, &snap).await {
                    warn!(?e, "re-register input overlay failed");
                }
                let cm = *self.runtime.capture_method.lock().await;
                let err_now = self.runtime.error.lock().await.clone();
                let sources = host.list_input_sources().await.unwrap_or_default();
                if let Err(e) = host
                    .settings_section_render(
                        SECTION_ID,
                        ui::settings_section(&snap, cm, err_now.as_deref(), &sources),
                    )
                    .await
                {
                    warn!(?e, "settings re-render after toggle failed");
                }
            }
            "tideline-ptt:set_mode_toggle_binding" | "tideline-ptt:set_hold_binding" => {
                let parsed: Option<Binding> = if value.is_null() {
                    None
                } else if let Some(s) = value.as_str() {
                    match Binding::parse(s) {
                        Ok(b) => Some(b),
                        Err(e) => {
                            warn!(input = s, error = %e, "failed to parse binding string");
                            return Ok(json!({}));
                        }
                    }
                } else {
                    match serde_json::from_value::<Binding>(value.clone()) {
                        Ok(b) => Some(b),
                        Err(e) => {
                            warn!(?value, error = %e, "failed to deserialize binding");
                            return Ok(json!({}));
                        }
                    }
                };
                let mut cfg = self.runtime.config.lock().await;
                let is_mode_toggle = event_id.ends_with("mode_toggle_binding");
                if is_mode_toggle {
                    cfg.mode_toggle_binding = parsed;
                } else {
                    cfg.hold_binding = parsed;
                }
                let snap = cfg.clone();
                drop(cfg);
                if let Err(e) = config::save(&host, PLUGIN_ID, &snap).await {
                    error!(?e, "config save failed");
                }
                if let Err(e) = push_input_overlay(&host, &snap).await {
                    warn!(?e, "re-register input overlay after binding change failed");
                }
                let cm = *self.runtime.capture_method.lock().await;
                let err_now = self.runtime.error.lock().await.clone();
                let sources = host.list_input_sources().await.unwrap_or_default();
                if let Err(e) = host
                    .settings_section_render(
                        SECTION_ID,
                        ui::settings_section(&snap, cm, err_now.as_deref(), &sources),
                    )
                    .await
                {
                    warn!(?e, "settings re-render after binding change failed");
                }
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

async fn push_input_overlay(
    host: &Arc<HostClient>,
    cfg: &PluginConfig,
) -> Result<(), tideline_sdk::transport::SdkTransportError> {
    let mut values_by_source = serde_json::Map::new();
    for src in &cfg.enabled_sources {
        let mut per: serde_json::Map<String, Value> = serde_json::Map::new();
        per.insert("tideline-ptt:input_enabled".into(), Value::Bool(true));
        values_by_source.insert(src.clone(), Value::Object(per));
    }
    host.register_input_overlay(json!({
        "surface_id": INPUT_OVERLAY_ID,
        "input_filter": { "kind": "physical_only" },
        "tree": {
            "kind": "section",
            "id": "tideline-ptt-input-overlay",
            "children": [
                {
                    "kind": "toggle",
                    "id": "tideline-ptt:input_enabled",
                    "label": "Push to Talk",
                    "sublabel": "Mute this input until you press the PTT key",
                    "value": false,
                }
            ]
        },
        "values_by_source": Value::Object(values_by_source),
    }))
    .await
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
    let _log_guard = tideline_sdk::logging::init("tideline-ptt");

    let runtime = PttRuntime::new_without_client(PLUGIN_ID, PluginConfig::default());
    let plugin = PttPlugin { runtime };
    run(plugin).await;
}
