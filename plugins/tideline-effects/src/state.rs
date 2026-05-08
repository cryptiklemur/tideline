//! In-process state for the effects plugin.
//!
//! - `engine` — the AudioEngine, set once on plugin startup.
//! - `chains` — per-channel effect order (`Vec<Uuid>`).
//! - `effects` — full Effect data keyed by (channel_id, effect_id).
//! - `host` — the deferred HostClient injected from on_ready.
//! - `catalog` — cached PluginInfo list from FormatRegistry::scan_all.

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use serde_json::Value;
use tokio::sync::{Mutex, OnceCell};
use uuid::Uuid;

use tideline_sdk::HostClient;

use crate::effect::Effect;
use crate::engine::AudioEngine;
use crate::host::{Format, PluginInfo};

#[derive(Debug, Clone)]
pub struct ChainSlot {
    pub effect: Effect,
}

pub struct EffectsState {
    #[allow(dead_code)]
    pub plugin_id: &'static str,
    pub host: OnceCell<Arc<HostClient>>,
    pub engine_cell: OnceCell<Arc<AudioEngine>>,
    pub chains: Mutex<HashMap<Uuid, Vec<Uuid>>>,
    pub effects: Mutex<BTreeMap<(Uuid, Uuid), ChainSlot>>,
    pub chain_bypass: Mutex<HashMap<Uuid, bool>>,
    pub catalog: Mutex<Vec<PluginInfo>>,
    /// When true, mutations skip writing chains.json. Used during
    /// `apply_persisted_chains` so a partial reload can't overwrite a
    /// known-good on-disk snapshot with an empty one.
    pub suppress_save: std::sync::atomic::AtomicBool,
    /// Set after the first `pipewire.contribute_request` reconciles engine
    /// state from `AppConfig.plugin_data`. Guards against repeated full
    /// rebuilds on every subsequent contribute call.
    pub appconfig_synced: std::sync::atomic::AtomicBool,
    /// Set once `set_catalog` has been called with the LV2 plugin catalog.
    /// Contribute requests can race ahead of `on_ready`'s discovery scan;
    /// the contributor must `wait_for_catalog().await` before any
    /// `apply_persisted_chains` call so `add_effect` finds the plugin URI
    /// in the catalog instead of failing with "plugin not in catalog".
    pub catalog_ready: std::sync::atomic::AtomicBool,
    pub catalog_ready_notify: tokio::sync::Notify,
    /// Per-session FX-audition session (one channel at a time). Holds
    /// the recorded sample, recording/loop-player processes, and the
    /// channel slug that drives the temp `audition_source.*` virtual
    /// source name. Ephemeral — never written to disk.
    pub audition: crate::audition::AuditionStore,
}

impl EffectsState {
    pub fn new(plugin_id: &'static str) -> Arc<Self> {
        Arc::new(Self {
            plugin_id,
            host: OnceCell::new(),
            engine_cell: OnceCell::new(),
            chains: Mutex::new(HashMap::new()),
            effects: Mutex::new(BTreeMap::new()),
            chain_bypass: Mutex::new(HashMap::new()),
            catalog: Mutex::new(Vec::new()),
            suppress_save: std::sync::atomic::AtomicBool::new(false),
            appconfig_synced: std::sync::atomic::AtomicBool::new(false),
            catalog_ready: std::sync::atomic::AtomicBool::new(false),
            catalog_ready_notify: tokio::sync::Notify::new(),
            audition: crate::audition::AuditionStore::new(),
        })
    }

    pub fn set_host(&self, host: Arc<HostClient>) {
        let _ = self.host.set(host);
    }

    pub fn set_engine(&self, engine: Arc<AudioEngine>) {
        let _ = self.engine_cell.set(engine);
    }

    pub fn engine(&self) -> Option<Arc<AudioEngine>> {
        self.engine_cell.get().cloned()
    }

    pub async fn set_catalog(&self, infos: Vec<PluginInfo>) {
        *self.catalog.lock().await = infos;
        self.catalog_ready
            .store(true, std::sync::atomic::Ordering::Release);
        self.catalog_ready_notify.notify_waiters();
    }

    /// Block until `set_catalog` has run at least once. Used by the
    /// pipewire contributor: contribute_request RPCs can land before
    /// `discovery::run_first_boot` populates the catalog, so the
    /// contributor's first-call `apply_persisted_chains` would otherwise
    /// see an empty catalog and fail to attach every persisted effect.
    pub async fn wait_for_catalog(&self) {
        loop {
            if self.catalog_ready.load(std::sync::atomic::Ordering::Acquire) {
                return;
            }
            let notified = self.catalog_ready_notify.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            if self.catalog_ready.load(std::sync::atomic::Ordering::Acquire) {
                return;
            }
            notified.await;
        }
    }

    pub async fn catalog_clone(&self) -> Vec<PluginInfo> {
        self.catalog.lock().await.clone()
    }

    pub fn find_catalog_entry(&self, format: Format, uri: &str) -> Option<PluginInfo> {
        let guard = self.catalog.try_lock().ok()?;
        guard
            .iter()
            .find(|i| i.format == format && i.uri == uri)
            .cloned()
    }

    pub async fn attach_effect(&self, channel_id: Uuid, effect: Effect) {
        let eid = effect.id;
        {
            self.effects
                .lock()
                .await
                .insert((channel_id, eid), ChainSlot { effect });
        }
        {
            let mut chains = self.chains.lock().await;
            let order = chains.entry(channel_id).or_default();
            if !order.contains(&eid) {
                order.push(eid);
            }
        }
        crate::persist::save_chains_to_disk(self).await;
    }

    pub async fn detach_effect(&self, channel_id: Uuid, effect_id: Uuid) {
        {
            self.effects.lock().await.remove(&(channel_id, effect_id));
        }
        {
            if let Some(chain) = self.chains.lock().await.get_mut(&channel_id) {
                chain.retain(|e| *e != effect_id);
            }
        }
        crate::persist::save_chains_to_disk(self).await;
    }

    pub async fn chain_order(&self, channel_id: Uuid) -> Vec<Uuid> {
        self.chains
            .lock()
            .await
            .get(&channel_id)
            .cloned()
            .unwrap_or_default()
    }

    pub async fn set_chain_order(&self, channel_id: Uuid, order: Vec<Uuid>) {
        {
            self.chains.lock().await.insert(channel_id, order);
        }
        crate::persist::save_chains_to_disk(self).await;
    }

    pub async fn channels_with_effects(&self) -> Vec<Uuid> {
        self.chains
            .lock()
            .await
            .iter()
            .filter(|(_, v)| !v.is_empty())
            .map(|(k, _)| *k)
            .collect()
    }

    pub async fn set_effect_bypassed(
        &self,
        channel_id: Uuid,
        effect_id: Uuid,
        bypassed: bool,
    ) {
        {
            let mut effects = self.effects.lock().await;
            if let Some(slot) = effects.get_mut(&(channel_id, effect_id)) {
                slot.effect.bypassed = bypassed;
            }
        }
        if let Some(engine) = self.engine() {
            let _ = engine.set_bypass(channel_id, effect_id, bypassed);
        }
        crate::persist::save_chains_to_disk(self).await;
    }

    pub async fn set_effect_state_b64(
        &self,
        channel_id: Uuid,
        effect_id: Uuid,
        b64: String,
    ) {
        {
            let mut effects = self.effects.lock().await;
            if let Some(slot) = effects.get_mut(&(channel_id, effect_id)) {
                slot.effect.state_b64 = Some(b64);
            }
        }
        crate::persist::save_chains_to_disk(self).await;
    }

    pub async fn get_chain_bypass(&self, channel_id: Uuid) -> bool {
        self.chain_bypass
            .lock()
            .await
            .get(&channel_id)
            .copied()
            .unwrap_or(false)
    }

    pub async fn set_chain_bypass(&self, channel_id: Uuid, bypassed: bool) {
        {
            self.chain_bypass.lock().await.insert(channel_id, bypassed);
        }
        if let Some(engine) = self.engine() {
            let _ = engine.set_chain_bypass(channel_id, bypassed);
        }
        crate::persist::save_chains_to_disk(self).await;
    }

    pub async fn snapshot_persisted(&self) -> crate::persist::PersistedChains {
        let chains = self.chains.lock().await.clone();
        let effects = self.effects.lock().await.clone();
        let chain_bypass = self.chain_bypass.lock().await.clone();
        let mut channels: BTreeMap<Uuid, crate::persist::PersistedChannel> = BTreeMap::new();
        for (channel_id, order) in chains {
            let mut effect_list: Vec<Effect> = Vec::with_capacity(order.len());
            for eid in order {
                if let Some(slot) = effects.get(&(channel_id, eid)) {
                    effect_list.push(slot.effect.clone());
                }
            }
            channels.insert(
                channel_id,
                crate::persist::PersistedChannel {
                    bypassed: chain_bypass.get(&channel_id).copied().unwrap_or(false),
                    effects: effect_list,
                },
            );
        }
        for (channel_id, bypassed) in chain_bypass {
            channels.entry(channel_id).or_insert(crate::persist::PersistedChannel {
                bypassed,
                effects: Vec::new(),
            });
        }
        crate::persist::PersistedChains { version: 2, channels }
    }

    pub async fn clear_chain_state(&self) {
        self.chains.lock().await.clear();
        self.effects.lock().await.clear();
    }
}

pub async fn on_channel_removed(state: Arc<EffectsState>, params: Option<Value>) {
    let Some(channel_id) = params
        .as_ref()
        .and_then(|v| v.get("channel_id"))
        .and_then(|v| v.as_str())
        .and_then(|s| Uuid::parse_str(s).ok())
    else {
        return;
    };
    let effect_ids: Vec<Uuid> = state.chain_order(channel_id).await;
    for eid in effect_ids {
        let _ = crate::chain_ops::remove_effect(state.clone(), channel_id, eid).await;
    }
    if let Some(engine) = state.engine() {
        engine.remove_channel(channel_id);
    }
    {
        state.chains.lock().await.remove(&channel_id);
    }
    {
        state.chain_bypass.lock().await.remove(&channel_id);
    }
    state.audition.discard(Some(channel_id)).await;
    crate::persist::save_chains_to_disk(&state).await;
}

pub async fn apply_persisted_chains(
    state: Arc<EffectsState>,
    persisted: crate::persist::PersistedChains,
) {
    let total_chains = persisted.channels.len();
    let total_effects: usize = persisted.channels.values().map(|c| c.effects.len()).sum();
    tracing::info!(total_chains, total_effects, "apply_persisted_chains: starting");
    state
        .suppress_save
        .store(true, std::sync::atomic::Ordering::Relaxed);
    state.clear_chain_state().await;
    let mut any_failed = false;
    for (channel_id, ch) in persisted.channels {
        state.set_chain_bypass(channel_id, ch.bypassed).await;
        // Dedupe by effect id before applying. attach_effect will skip
        // duplicates in chain_order, but engine.add_plugin would still
        // be called twice and could enter a bad state. Persisted files
        // from older bug-prone builds can carry the same id twice.
        let mut seen = std::collections::HashSet::new();
        let mut deduped: Vec<crate::effect::Effect> = Vec::with_capacity(ch.effects.len());
        for effect in ch.effects {
            if seen.insert(effect.id) {
                deduped.push(effect);
            } else {
                tracing::warn!(?channel_id, effect_id = %effect.id, "apply_persisted_chains: dropping duplicate effect entry");
            }
        }
        for effect in deduped {
            let effect_id = effect.id;
            let bypassed = effect.bypassed;
            tracing::info!(?channel_id, ?effect_id, "applying persisted effect");
            match crate::chain_ops::add_effect(state.clone(), channel_id, effect).await {
                Ok(()) => {
                    if bypassed {
                        state.set_effect_bypassed(channel_id, effect_id, true).await;
                    }
                }
                Err(e) => {
                    any_failed = true;
                    tracing::warn!(error = %e, ?channel_id, ?effect_id, "reapply add_effect failed");
                }
            }
        }
    }
    state
        .suppress_save
        .store(false, std::sync::atomic::Ordering::Relaxed);
    if any_failed {
        tracing::warn!("apply_persisted_chains: some effects failed to reload — leaving chains.json untouched to avoid data loss");
    } else {
        crate::persist::save_chains_to_disk(&state).await;
    }
    tracing::info!("apply_persisted_chains: done");
}

/// Tear down every engine-side resource and rebuild it from current
/// in-memory `EffectsState`. Called when pipewire restarts and our JACK
/// clients are stale. Does NOT modify `EffectsState` or touch chains.json.
pub async fn recreate_engine_from_state(state: Arc<EffectsState>) {
    let Some(engine) = state.engine() else {
        tracing::warn!("recreate_engine_from_state: no engine");
        return;
    };
    let snapshot = state.snapshot_persisted().await;
    let total_effects: usize = snapshot.channels.values().map(|c| c.effects.len()).sum();
    tracing::info!(
        channels = snapshot.channels.len(),
        total_effects,
        "recreate_engine_from_state: rebuilding engine"
    );
    engine.clear_all_channels();
    state
        .suppress_save
        .store(true, std::sync::atomic::Ordering::Relaxed);
    for (channel_id, ch) in &snapshot.channels {
        let slug = crate::chain_ops::slug_for_channel(*channel_id);
        if let Err(e) = engine.ensure_channel(*channel_id, &slug) {
            tracing::warn!(error = %e, ?channel_id, "recreate: ensure_channel failed");
            continue;
        }
        if ch.bypassed {
            let _ = engine.set_chain_bypass(*channel_id, true);
        }
        for effect in &ch.effects {
            let info = match crate::chain_ops::plugin_info_for(&state, effect) {
                Ok(i) => i,
                Err(e) => {
                    tracing::warn!(error = %e, ?effect.id, "recreate: plugin_info_for failed");
                    continue;
                }
            };
            let state_blob = crate::chain_ops::decode_state(&effect.state_b64);
            if let Err(e) = engine.add_plugin(
                *channel_id,
                effect.id,
                &info,
                state_blob.as_deref(),
            ) {
                tracing::warn!(error = %e, ?channel_id, ?effect.id, "recreate: add_plugin failed");
                continue;
            }
            if effect.bypassed {
                let _ = engine.set_bypass(*channel_id, effect.id, true);
            }
        }
    }
    state
        .suppress_save
        .store(false, std::sync::atomic::Ordering::Relaxed);
    tracing::info!("recreate_engine_from_state: done");
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
        state.attach_effect(channel, effect).await;
        assert_eq!(state.chain_order(channel).await, vec![eid]);
        state.detach_effect(channel, eid).await;
        assert!(state.chain_order(channel).await.is_empty());
    }

    #[tokio::test]
    async fn channels_with_effects_filters_empty() {
        let state = EffectsState::new("test");
        let c1 = Uuid::new_v4();
        let _c2 = Uuid::new_v4();
        let e = Effect::new_lv2("uri");
        state.attach_effect(c1, e).await;
        let mut got = state.channels_with_effects().await;
        got.sort();
        assert_eq!(got, vec![c1]);
    }
}