//! Host-side backend hooks for native primitives the dispatcher cannot
//! implement on its own (mute, notify, etc.). The default `NullBackend`
//! returns `Ok(())` for every call so unit tests and headless harnesses
//! work without OS plumbing. The Tauri shell injects a real implementation
//! at startup via [`PluginRegistry::set_backend`].

use async_trait::async_trait;
use std::sync::Arc;

#[async_trait]
pub trait HostBackend: Send + Sync {
    /// Mute or unmute a PipeWire/PulseAudio source by node name.
    async fn set_source_mute(&self, node: &str, muted: bool) -> Result<(), String>;

    /// Surface a desktop notification.
    async fn notify(&self, title: &str, body: &str) -> Result<(), String>;
}

pub struct NullBackend;

#[async_trait]
impl HostBackend for NullBackend {
    async fn set_source_mute(&self, _node: &str, _muted: bool) -> Result<(), String> {
        Ok(())
    }
    async fn notify(&self, _title: &str, _body: &str) -> Result<(), String> {
        Ok(())
    }
}

pub fn null_backend() -> Arc<dyn HostBackend> {
    Arc::new(NullBackend)
}
