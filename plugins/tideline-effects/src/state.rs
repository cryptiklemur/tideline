//! In-process state for the effects plugin.
//!
//! - `engine` — the single carla Host wrapped in a tokio Mutex (carla is single-instance per process).
//! - `chains` — per-channel effect order (`Vec<Uuid>`) and effect→carla-plugin-id resolution.
//! - `effects` — full Effect data keyed by (channel_id, effect_id).
//! - `host` — the deferred HostClient injected from on_ready.

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use serde_json::Value;
use tokio::sync::{Mutex, OnceCell};
use uuid::Uuid;

use tideline_sdk::HostClient;

use crate::carla::Host;
use crate::effect::Effect;

#[derive(Debug, Clone)]
pub struct ChainSlot {
    pub effect: Effect,
    pub plugin_id: u32,
}

pub struct EffectsState {
    #[allow(dead_code)]
    pub plugin_id: &'static str,
    pub host: OnceCell<Arc<HostClient>>,
    pub engine: Mutex<Option<Arc<Mutex<Host>>>>,
    /// Effect chain order per channel — vec of effect ids in execution order.
    pub chains: Mutex<HashMap<Uuid, Vec<Uuid>>>,
    /// Concrete effect + carla plugin id, keyed (channel_id, effect_id).
    pub effects: Mutex<BTreeMap<(Uuid, Uuid), ChainSlot>>,
}

impl EffectsState {
    pub fn new(plugin_id: &'static str) -> Arc<Self> {
        Arc::new(Self {
            plugin_id,
            host: OnceCell::new(),
            engine: Mutex::new(None),
            chains: Mutex::new(HashMap::new()),
            effects: Mutex::new(BTreeMap::new()),
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

    pub async fn attach_effect(&self, channel_id: Uuid, effect: Effect, plugin_id: u32) {
        let eid = effect.id;
        self.effects.lock().await.insert((channel_id, eid), ChainSlot { effect, plugin_id });
        self.chains.lock().await.entry(channel_id).or_default().push(eid);
    }

    pub async fn detach_effect(&self, channel_id: Uuid, effect_id: Uuid) {
        self.effects.lock().await.remove(&(channel_id, effect_id));
        if let Some(chain) = self.chains.lock().await.get_mut(&channel_id) {
            chain.retain(|e| *e != effect_id);
        }
    }

    pub async fn lookup_plugin_id(&self, channel_id: Uuid, effect_id: Uuid) -> Option<u32> {
        self.effects.lock().await.get(&(channel_id, effect_id)).map(|s| s.plugin_id)
    }

    pub async fn chain_order(&self, channel_id: Uuid) -> Vec<Uuid> {
        self.chains.lock().await.get(&channel_id).cloned().unwrap_or_default()
    }

    pub async fn set_chain_order(&self, channel_id: Uuid, order: Vec<Uuid>) {
        self.chains.lock().await.insert(channel_id, order);
    }

    pub async fn channels_with_effects(&self) -> Vec<Uuid> {
        self.chains.lock().await.iter()
            .filter(|(_, v)| !v.is_empty())
            .map(|(k, _)| *k)
            .collect()
    }
}

pub async fn on_channel_removed(state: Arc<EffectsState>, params: Option<Value>) {
    let Some(channel_id) = params.as_ref()
        .and_then(|v| v.get("channel_id"))
        .and_then(|v| v.as_str())
        .and_then(|s| Uuid::parse_str(s).ok())
    else { return; };
    // Remove chain entries; engine-side cleanup happens via chain_ops::remove_effect calls
    // dispatched here.
    let effect_ids: Vec<Uuid> = state.chain_order(channel_id).await;
    for eid in effect_ids {
        let _ = crate::chain_ops::remove_effect(state.clone(), channel_id, eid).await;
    }
    state.chains.lock().await.remove(&channel_id);
}

pub async fn reapply_all_chains(state: Arc<EffectsState>) {
    // Called after engine restart: rebuild every chain in original order using
    // the persisted state_b64 from each Effect.
    let channels = state.channels_with_effects().await;
    for channel in channels {
        let order = state.chain_order(channel).await;
        for eid in order {
            let Some(slot) = state.effects.lock().await.get(&(channel, eid)).cloned() else { continue; };
            // Re-add as a fresh plugin instance (new carla id), then load_state if persisted.
            if let Ok(new_id) = crate::chain_ops::add_effect(state.clone(), channel, slot.effect.clone()).await {
                let _ = new_id;
                // TODO(T14b): restore plugin state via persist::load_effect_state
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn attach_then_detach() {
        let state = EffectsState::new("test");
        let channel = Uuid::new_v4();
        let effect = Effect::new_lv2("uri");
        let eid = effect.id;
        state.attach_effect(channel, effect, 0).await;
        assert_eq!(state.chain_order(channel).await, vec![eid]);
        assert_eq!(state.lookup_plugin_id(channel, eid).await, Some(0));
        state.detach_effect(channel, eid).await;
        assert!(state.lookup_plugin_id(channel, eid).await.is_none());
        assert!(state.chain_order(channel).await.is_empty());
    }

    #[tokio::test]
    async fn channels_with_effects_filters_empty() {
        let state = EffectsState::new("test");
        let c1 = Uuid::new_v4();
        let c2 = Uuid::new_v4();
        let e = Effect::new_lv2("uri");
        state.attach_effect(c1, e, 0).await;
        let mut got = state.channels_with_effects().await;
        got.sort();
        let mut want = vec![c1];
        want.sort();
        assert_eq!(got, want);
        let _ = c2; // c2 has no effects, should not appear
    }
}
