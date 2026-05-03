//! In-process channel-effects state. Filled in incrementally; T15 finishes.
use std::sync::Arc;
use tideline_sdk::HostClient;
use tokio::sync::Mutex;

use crate::carla::Host;

pub struct EffectsState {
    #[allow(dead_code)]
    pub plugin_id: &'static str,
    pub host: tokio::sync::OnceCell<Arc<HostClient>>,
    pub engine: Mutex<Option<Arc<Mutex<Host>>>>,
}

impl EffectsState {
    pub fn new(plugin_id: &'static str) -> Arc<Self> {
        Arc::new(Self {
            plugin_id,
            host: tokio::sync::OnceCell::new(),
            engine: Mutex::new(None),
        })
    }

    pub fn set_host(&self, host: Arc<HostClient>) {
        let _ = self.host.set(host);
    }

    pub async fn set_engine(&self, h: Arc<Mutex<Host>>) {
        *self.engine.lock().await = Some(h);
    }

    pub async fn take_engine(&self) -> Option<Arc<Mutex<Host>>> {
        self.engine.lock().await.take()
    }

    pub async fn engine(&self) -> Option<Arc<Mutex<Host>>> {
        self.engine.lock().await.clone()
    }
}

pub async fn on_channel_removed(_state: Arc<EffectsState>, _params: serde_json::Value) {
    // Filled in by Task 15.
}

/// Re-applies every saved channel chain after the engine restarts.
/// Real implementation lands in W7.T15.
pub async fn reapply_all_chains(_state: Arc<EffectsState>) {
    // W7.T15 — iterate persisted chains and call chain_ops::apply for each.
}
