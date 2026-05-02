use crate::config::{self, PluginConfig};
use crate::mute;
use crate::state::{self, Effects, Mode, PttState};
use serde::Serialize;
use serde_json::json;
use std::sync::Arc;
use tideline_sdk::HostClient;
use tokio::sync::{Mutex, OnceCell};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum CaptureMethod {
    #[default]
    None,
    Portal,
    Evdev,
}

pub struct PttRuntime {
    pub client: OnceCell<Arc<HostClient>>,
    pub namespace: &'static str,
    pub state: Mutex<PttState>,
    pub config: Mutex<PluginConfig>,
    pub capture_method: Mutex<CaptureMethod>,
    pub error: Mutex<Option<String>>,
}

impl PttRuntime {
    pub fn new_without_client(namespace: &'static str, config: PluginConfig) -> Arc<Self> {
        let state = PttState {
            mode: config.mode,
            hold_active: false,
        };
        Arc::new(Self {
            client: OnceCell::new(),
            namespace,
            state: Mutex::new(state),
            config: Mutex::new(config),
            capture_method: Mutex::new(CaptureMethod::None),
            error: Mutex::new(None),
        })
    }

    pub fn set_host(&self, host: Arc<HostClient>) {
        let _ = self.client.set(host);
    }

    fn host(&self) -> Option<&Arc<HostClient>> {
        self.client.get()
    }

    pub async fn toggle_mode(self: &Arc<Self>) {
        let fx = { state::toggle_mode(&mut *self.state.lock().await) };
        self.apply_effects(fx).await;
    }

    pub async fn hold_press(self: &Arc<Self>) {
        let fx = { state::hold_press(&mut *self.state.lock().await) };
        self.apply_effects(fx).await;
    }

    pub async fn hold_release(self: &Arc<Self>) {
        let fx = { state::hold_release(&mut *self.state.lock().await) };
        self.apply_effects(fx).await;
    }

    async fn apply_effects(&self, fx: Effects) {
        let Some(host) = self.host() else {
            tracing::warn!("apply_effects called before host set");
            return;
        };
        let cfg = self.config.lock().await.clone();

        if let Some(muted) = fx.set_muted {
            if let Some(node) = cfg.input_device.as_deref() {
                if let Err(e) = mute::set_source_mute(host, node, muted).await {
                    tracing::warn!(?e, "set_source_mute failed");
                }
            }
        }

        if let Some(mode) = fx.mode_changed {
            let payload = json!({ "mode": mode });
            if let Err(e) = host.event_publish("tideline-ptt:mode_changed", payload).await {
                tracing::warn!(?e, "event_publish mode_changed failed");
            }
        }

        if let Some(mode) = fx.notify_mode {
            let title = match mode {
                Mode::Open => "Mic — open",
                Mode::Ptt => "Mic — push-to-talk",
            };
            if let Err(e) = host.notify(title, "").await {
                tracing::warn!(?e, "notify failed");
            }
        }

        if let Some(tx) = fx.transmit_changed {
            let payload = json!({ "transmitting": tx });
            if let Err(e) = host
                .event_publish("tideline-ptt:transmit_changed", payload)
                .await
            {
                tracing::warn!(?e, "event_publish transmit_changed failed");
            }
        }

        if let Some(mode) = fx.persist_mode {
            let mut cfg_lock = self.config.lock().await;
            cfg_lock.mode = mode;
            let snapshot = cfg_lock.clone();
            drop(cfg_lock);
            if let Err(e) = config::save(host, self.namespace, &snapshot).await {
                tracing::warn!(?e, "config save failed");
            }
        }

        let s = *self.state.lock().await;
        let cm = *self.capture_method.lock().await;
        let err = self.error.lock().await.clone();
        let payload = json!({
            "mode": s.mode,
            "hold_active": s.hold_active,
            "transmitting": s.transmitting(),
            "capture_method": cm,
            "error": err,
        });
        if let Err(e) = host.event_publish("tideline-ptt:state_changed", payload).await {
            tracing::warn!(?e, "event_publish state_changed failed");
        }
    }

    pub async fn set_capture_method(&self, m: CaptureMethod) {
        *self.capture_method.lock().await = m;
    }

    pub async fn set_error(&self, e: Option<String>) {
        *self.error.lock().await = e;
    }
}
