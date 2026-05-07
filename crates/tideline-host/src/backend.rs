//! Host-side backend hooks for native primitives the dispatcher cannot
//! implement on its own (mute, notify, etc.). The default `NullBackend`
//! returns `Ok(())` for every call so unit tests and headless harnesses
//! work without OS plumbing. The Tauri shell injects a real implementation
//! at startup via [`PluginRegistry::set_backend`].

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioSource {
    pub name: String,
    pub description: String,
}

#[async_trait]
pub trait HostBackend: Send + Sync {
    async fn set_source_mute(&self, node: &str, muted: bool) -> Result<(), String>;

    async fn notify(&self, title: &str, body: &str) -> Result<(), String>;

    async fn list_input_sources(&self) -> Result<Vec<AudioSource>, String>;

    async fn config_namespace_get(&self, namespace: &str) -> Result<serde_json::Value, String>;

    async fn config_namespace_set(
        &self,
        namespace: &str,
        value: serde_json::Value,
    ) -> Result<(), String>;

    /// Attach plugin-owned data onto a channel's `plugin_data[namespace]` slot
    /// and persist the config. Used by plugins to record per-channel state
    /// that downstream contributors (pipewire, etc.) read back from the
    /// AppConfig snapshot.
    async fn attach_channel_data(
        &self,
        namespace: &str,
        channel_uuid: uuid::Uuid,
        value: serde_json::Value,
    ) -> Result<(), String>;
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
    async fn list_input_sources(&self) -> Result<Vec<AudioSource>, String> {
        Ok(Vec::new())
    }
    async fn config_namespace_get(
        &self,
        _namespace: &str,
    ) -> Result<serde_json::Value, String> {
        Ok(serde_json::Value::Null)
    }
    async fn config_namespace_set(
        &self,
        _namespace: &str,
        _value: serde_json::Value,
    ) -> Result<(), String> {
        Ok(())
    }
    async fn attach_channel_data(
        &self,
        _namespace: &str,
        _channel_uuid: uuid::Uuid,
        _value: serde_json::Value,
    ) -> Result<(), String> {
        Ok(())
    }
}

pub fn null_backend() -> Arc<dyn HostBackend> {
    Arc::new(NullBackend)
}
