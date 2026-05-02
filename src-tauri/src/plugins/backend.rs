//! Concrete [`HostBackend`] implementation for the Tauri shell. Wires plugin
//! RPC calls (`host/source.set_mute`, `host/notify`) to the OS-level facilities
//! the headless host crate cannot reach: pactl for muting, notify-rust for
//! desktop toasts.

use async_trait::async_trait;
use std::process::Command;
use tideline_host::backend::HostBackend;

pub struct TauriHostBackend;

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
        let node = node.to_string();
        tokio::task::spawn_blocking(move || pactl_set_source_mute(&node, muted))
            .await
            .map_err(|e| format!("mute task join failed: {e}"))?
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
}
