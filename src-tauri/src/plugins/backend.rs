//! Concrete [`HostBackend`] implementation for the Tauri shell. Wires plugin
//! RPC calls (`host/source.set_mute`, `host/notify`) to the OS-level facilities
//! the headless host crate cannot reach: pactl for muting, notify-rust for
//! desktop toasts.

use async_trait::async_trait;
use std::process::Command;
use tauri::{AppHandle, Emitter, Manager, Wry};
use tideline_host::backend::HostBackend;

pub struct TauriHostBackend {
    app: AppHandle<Wry>,
}

impl TauriHostBackend {
    pub fn new(app: AppHandle<Wry>) -> Self {
        Self { app }
    }
}

fn pactl_set_source_mute(node: &str, muted: bool) -> Result<(), String> {
    if node.is_empty() {
        return Err("source node not configured".into());
    }
    let out = Command::new("pactl")
        .args(["set-source-mute", node, if muted { "1" } else { "0" }])
        .output()
        .map_err(|e| format!("pactl exec failed: {}", e))?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).trim().to_string());
    }
    Ok(())
}

#[async_trait]
impl HostBackend for TauriHostBackend {
    async fn set_source_mute(&self, node: &str, muted: bool) -> Result<(), String> {
        let node_owned = node.to_string();
        let res = tokio::task::spawn_blocking(move || pactl_set_source_mute(&node_owned, muted))
            .await
            .map_err(|e| format!("mute task join failed: {e}"))?;
        if res.is_ok() {
            let _ = self.app.emit(
                "tideline:source_mute_changed",
                serde_json::json!({ "source_name": node, "muted": muted }),
            );
        }
        res
    }

    async fn notify(&self, title: &str, body: &str) -> Result<(), String> {
        let title = title.to_string();
        let body = body.to_string();
        tokio::task::spawn_blocking(move || {
            notify_rust::Notification::new()
                .summary(&title)
                .body(&body)
                .appname("Tideline")
                .timeout(notify_rust::Timeout::Milliseconds(3000))
                .show()
                .map(|_| ())
                .map_err(|e| format!("notify-rust failed: {e}"))
        })
        .await
        .map_err(|e| format!("notify task join failed: {e}"))?
    }

    async fn list_input_sources(&self) -> Result<Vec<tideline_host::backend::AudioSource>, String> {
        tokio::task::spawn_blocking(|| {
            crate::fetch_sources()
                .into_iter()
                .filter(|s| !s.name.ends_with(".monitor"))
                .map(|s| tideline_host::backend::AudioSource {
                    name: s.name,
                    description: s.description,
                })
                .collect()
        })
        .await
        .map_err(|e| format!("list sources task join failed: {e}"))
    }

    async fn config_namespace_get(
        &self,
        namespace: &str,
    ) -> Result<serde_json::Value, String> {
        let namespace = namespace.to_string();
        tokio::task::spawn_blocking(move || {
            let cfg = tideline_core::config_io::load_config();
            Ok(cfg
                .plugin_data
                .get(&namespace)
                .cloned()
                .unwrap_or(serde_json::Value::Null))
        })
        .await
        .map_err(|e| format!("config_namespace_get join failed: {e}"))?
    }

    async fn config_namespace_set(
        &self,
        namespace: &str,
        value: serde_json::Value,
    ) -> Result<(), String> {
        let namespace = namespace.to_string();
        tokio::task::spawn_blocking(move || {
            let mut cfg = tideline_core::config_io::load_config();
            cfg.plugin_data.insert(namespace, value);
            tideline_core::config_io::save_config_to_disk(&cfg)
        })
        .await
        .map_err(|e| format!("config_namespace_set join failed: {e}"))?
    }

    async fn attach_channel_data(
        &self,
        namespace: &str,
        channel_uuid: uuid::Uuid,
        value: serde_json::Value,
    ) -> Result<(), String> {
        let namespace = namespace.to_string();
        let app = self.app.clone();
        let app_for_pw = self.app.clone();
        let namespace_for_pw = namespace.clone();
        tokio::task::spawn_blocking(move || -> Result<(), String> {
            // Source of truth is the app's in-memory AppState. Without
            // mutating it here, downstream consumers (write_pipewire_and_restart)
            // read a stale snapshot and ignore the just-attached plugin_data.
            let state = app
                .try_state::<crate::AppState>()
                .ok_or_else(|| "attach_channel_data: AppState missing".to_string())?;
            {
                let mut cfg = state.config.lock().unwrap();
                let Some(channel) = cfg.channels.iter_mut().find(|c| c.uuid == channel_uuid) else {
                    return Err(format!(
                        "attach_channel_data: channel {channel_uuid} not found"
                    ));
                };
                channel.plugin_data.insert(namespace.clone(), value);
                tideline_core::config_io::save_config_to_disk(&cfg)?;
            }
            let _ = app.emit(
                "tideline:channel_plugin_data_changed",
                serde_json::json!({
                    "channel_uuid": channel_uuid,
                    "namespace": namespace,
                }),
            );
            Ok(())
        })
        .await
        .map_err(|e| format!("attach_channel_data join failed: {e}"))??;

        // Direct rebuild trigger: if the calling plugin contributes to
        // pipewire, nudge the debounced rebuild worker so its conf is
        // regenerated promptly. Independent of the rack_changed event bus
        // path; both converge on the same Notify.
        if let Some(registry) = app_for_pw.try_state::<std::sync::Arc<tideline_host::PluginRegistry>>() {
            let contributors = registry
                .plugins_with_capability(tideline_sdk::Capability::PipewireContribute)
                .await;
            if contributors.iter().any(|id| id == &namespace_for_pw) {
                if let Some(trigger) = app_for_pw.try_state::<crate::PipewireRebuildTrigger>() {
                    eprintln!(
                        "[pipewire] attach_channel_data poking rebuild for plugin '{namespace_for_pw}'"
                    );
                    trigger.poke();
                }
            }
        }
        Ok(())
    }
}
