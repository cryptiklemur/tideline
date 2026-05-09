use crate::config::{self, PluginConfig};
use crate::mute;
use crate::state::{self, Effects, Mode, PerSourceState};
use serde::Serialize;
use serde_json::json;
use std::collections::HashMap;
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
    pub state_by_source: Mutex<HashMap<String, PerSourceState>>,
    pub config: Mutex<PluginConfig>,
    pub capture_method: Mutex<CaptureMethod>,
    pub error: Mutex<Option<String>>,
}

impl PttRuntime {
    pub fn new_without_client(namespace: &'static str, config: PluginConfig) -> Arc<Self> {
        let mut state_by_source: HashMap<String, PerSourceState> = HashMap::new();
        for src in &config.enabled_sources {
            let mode = config.mode_for(src);
            state_by_source.insert(
                src.clone(),
                PerSourceState {
                    mode,
                    hold_active: false,
                },
            );
        }
        Arc::new(Self {
            client: OnceCell::new(),
            namespace,
            state_by_source: Mutex::new(state_by_source),
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

    async fn ensure_source_state(&self, source: &str) -> PerSourceState {
        let mut map = self.state_by_source.lock().await;
        if let Some(s) = map.get(source) {
            return *s;
        }
        let mode = self.config.lock().await.mode_for(source);
        let s = PerSourceState {
            mode,
            hold_active: false,
        };
        map.insert(source.to_string(), s);
        s
    }

    pub async fn toggle_mode(self: &Arc<Self>, source: &str) {
        eprintln!("PTT: runtime.toggle_mode source={}", source);
        self.ensure_source_state(source).await;
        let fx = {
            let mut map = self.state_by_source.lock().await;
            let s = map
                .entry(source.to_string())
                .or_insert(PerSourceState::default());
            state::toggle_mode(s, source)
        };
        self.apply_effects(fx).await;
    }

    pub async fn hold_press(self: &Arc<Self>, source: &str) {
        eprintln!("PTT: runtime.hold_press source={}", source);
        self.ensure_source_state(source).await;
        let fx = {
            let mut map = self.state_by_source.lock().await;
            let s = map
                .entry(source.to_string())
                .or_insert(PerSourceState::default());
            state::hold_press(s, source)
        };
        self.apply_effects(fx).await;
    }

    pub async fn hold_release(self: &Arc<Self>, source: &str) {
        eprintln!("PTT: runtime.hold_release source={}", source);
        self.ensure_source_state(source).await;
        let fx = {
            let mut map = self.state_by_source.lock().await;
            let s = map
                .entry(source.to_string())
                .or_insert(PerSourceState::default());
            state::hold_release(s, source)
        };
        self.apply_effects(fx).await;
    }

    async fn apply_effects(&self, fx: Effects) {
        let Some(host) = self.host() else {
            tracing::warn!("apply_effects called before host set");
            return;
        };
        let source = fx.source_name.clone();

        if let Some(muted) = fx.set_muted {
            if let Err(e) = mute::set_source_mute(host, &source, muted).await {
                tracing::warn!(?e, source = %source, "set_source_mute failed");
            }
        }

        if let Some(mode) = fx.mode_changed {
            let payload = json!({ "mode": mode, "source_name": source });
            if let Err(e) = host
                .event_publish("tideline-ptt:mode_changed", payload)
                .await
            {
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
            let payload = json!({ "transmitting": tx, "source_name": source });
            if let Err(e) = host
                .event_publish("tideline-ptt:transmit_changed", payload)
                .await
            {
                tracing::warn!(?e, "event_publish transmit_changed failed");
            }
        }

        if let Some(mode) = fx.persist_mode {
            eprintln!("PTT: persist_mode fired source={} mode={:?}", source, mode);
            let mut cfg_lock = self.config.lock().await;
            cfg_lock.mode_by_source.insert(source.clone(), mode);
            let snapshot = cfg_lock.clone();
            drop(cfg_lock);
            match config::save(host, self.namespace, &snapshot).await {
                Ok(()) => eprintln!("PTT: config save OK source={} mode={:?}", source, mode),
                Err(e) => {
                    eprintln!(
                        "PTT: config save FAILED source={} mode={:?} err={:?}",
                        source, mode, e
                    );
                    tracing::warn!(?e, "config save failed");
                }
            }
        }

        let play_tone = fx.transmit_changed.map(|tx| if tx { "up" } else { "down" });
        eprintln!(
            "PTT: publishing state_changed source={} play_tone={:?} tx={:?}",
            source, play_tone, fx.transmit_changed
        );
        self.publish_state_for(&source, play_tone).await;
    }

    async fn publish_state_for(&self, source: &str, play_tone: Option<&'static str>) {
        let Some(host) = self.host() else {
            return;
        };
        let s = {
            let map = self.state_by_source.lock().await;
            map.get(source).copied().unwrap_or_default()
        };
        let cm = *self.capture_method.lock().await;
        let err = self.error.lock().await.clone();
        let mut payload = json!({
            "mode": s.mode,
            "hold_active": s.hold_active,
            "transmitting": s.transmitting(),
            "capture_method": cm,
            "error": err,
            "source_name": source,
        });
        if let Some(tone) = play_tone {
            payload
                .as_object_mut()
                .expect("json! produces object")
                .insert("play_tone".into(), serde_json::Value::String(tone.into()));
        }
        if let Err(e) = host
            .event_publish("tideline-ptt:state_changed", payload)
            .await
        {
            tracing::warn!(?e, "event_publish state_changed failed");
        }
    }

    pub async fn publish_all_states(&self) {
        let sources: Vec<String> = self.config.lock().await.enabled_sources.clone();
        for src in &sources {
            self.ensure_source_state(src).await;
            self.publish_state_for(src, None).await;
        }
    }

    pub async fn set_capture_method(&self, m: CaptureMethod) {
        *self.capture_method.lock().await = m;
    }

    pub async fn set_error(&self, e: Option<String>) {
        *self.error.lock().await = e;
    }

    /// Reapply per-source mute state to pipewire/pulse. Used after pipewire
    /// restarts under us, which resets every mute on the system. Without this
    /// the user sees their PTT UI claiming muted while the mic is actually live.
    pub async fn reapply_all_mutes(&self) {
        let Some(host) = self.host() else {
            tracing::warn!("reapply_all_mutes: host not set");
            return;
        };
        let states = self.state_by_source.lock().await.clone();
        for (source, st) in states {
            let muted = !st.transmitting();
            if let Err(e) = mute::set_source_mute(host, &source, muted).await {
                tracing::warn!(?e, source = %source, muted, "reapply mute failed");
            } else {
                tracing::info!(source = %source, muted, "reapplied mute after pipewire restart");
            }
        }
    }
}
