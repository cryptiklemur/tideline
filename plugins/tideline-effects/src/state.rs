//! In-process channel-effects state. Filled in incrementally; T15 finishes.
use std::sync::Arc;
use tideline_sdk::HostClient;
use tokio::sync::Mutex;
use uuid::Uuid;

use crate::carla::Host;
use crate::effect::Effect;

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

    pub async fn attach_effect(&self, _channel_id: Uuid, _effect: Effect, _plugin_id: u32) {
        // W7.T15 — push (effect_id → plugin_id) into per-channel map and chain order.
    }

    pub async fn detach_effect(&self, _channel_id: Uuid, _effect_id: Uuid) {
        // W7.T15 — remove from per-channel map + chain order.
    }

    pub async fn lookup_plugin_id(&self, _channel_id: Uuid, _effect_id: Uuid) -> Option<u32> {
        // W7.T15 — read from per-channel map. Stub returns None.
        None
    }

    pub async fn chain_order(&self, _channel_id: Uuid) -> Vec<Uuid> {
        // W7.T15 — return per-channel chain order vec. Stub returns empty.
        Vec::new()
    }

    pub async fn set_chain_order(&self, _channel_id: Uuid, _order: Vec<Uuid>) {
        // W7.T15 — replace chain order vec.
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
