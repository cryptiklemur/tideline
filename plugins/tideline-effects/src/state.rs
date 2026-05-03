//! In-process channel-effects state. Filled in by Task 15.
use std::sync::Arc;
use tideline_sdk::HostClient;

pub struct EffectsState {
    #[allow(dead_code)]
    pub plugin_id: &'static str,
    pub host: tokio::sync::OnceCell<Arc<HostClient>>,
}

impl EffectsState {
    pub fn new(plugin_id: &'static str) -> Arc<Self> {
        Arc::new(Self {
            plugin_id,
            host: tokio::sync::OnceCell::new(),
        })
    }

    pub fn set_host(&self, host: Arc<HostClient>) {
        let _ = self.host.set(host);
    }
}

pub async fn on_channel_removed(_state: Arc<EffectsState>, _params: serde_json::Value) {
    // Filled in by Task 15.
}
