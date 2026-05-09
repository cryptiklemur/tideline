use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;

use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use serde::{Deserialize, Serialize};
use tracing::warn;
use uuid::Uuid;

use crate::effect::Effect;
use crate::state::EffectsState;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersistedChannel {
    #[serde(default)]
    pub bypassed: bool,
    #[serde(default)]
    pub lowcut: bool,
    #[serde(default)]
    pub clipguard: bool,
    #[serde(default = "default_input_gain")]
    pub input_gain: f32,
    #[serde(default)]
    pub effects: Vec<Effect>,
}

impl Default for PersistedChannel {
    fn default() -> Self {
        Self {
            bypassed: false,
            lowcut: false,
            clipguard: false,
            input_gain: 1.0,
            effects: Vec::new(),
        }
    }
}

fn default_input_gain() -> f32 { 1.0 }

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct PersistedChains {
    #[serde(default = "default_version")]
    pub version: u32,
    #[serde(default)]
    pub channels: BTreeMap<Uuid, PersistedChannel>,
}

fn default_version() -> u32 {
    2
}

const CURRENT_VERSION: u32 = 2;

pub fn chains_path() -> PathBuf {
    let dir = dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join("tideline")
        .join("effects");
    let _ = std::fs::create_dir_all(&dir);
    dir.join("chains.json")
}

/// Migrate v1 (no `version`, no per-effect `format`) → v2. v1 files were
/// LV2-only, so missing format defaults to "lv2".
fn migrate_v1_value(mut value: serde_json::Value) -> serde_json::Value {
    if value.get("version").is_none() {
        if let Some(channels) = value.get_mut("channels").and_then(|c| c.as_object_mut()) {
            for (_, ch) in channels.iter_mut() {
                if let Some(effects) = ch.get_mut("effects").and_then(|e| e.as_array_mut()) {
                    for effect in effects {
                        if let Some(obj) = effect.as_object_mut() {
                            obj.entry("format")
                                .or_insert(serde_json::Value::String("lv2".into()));
                        }
                    }
                }
            }
        }
        if let Some(obj) = value.as_object_mut() {
            obj.insert("version".into(), serde_json::json!(CURRENT_VERSION));
        }
    }
    value
}

pub fn load_chains_from_disk() -> PersistedChains {
    let path = chains_path();
    let bytes = match std::fs::read(&path) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return PersistedChains::default(),
        Err(e) => {
            warn!(error = %e, path = %path.display(), "load_chains_from_disk: read failed");
            return PersistedChains::default();
        }
    };
    let raw: serde_json::Value = match serde_json::from_slice(&bytes) {
        Ok(v) => v,
        Err(e) => {
            warn!(error = %e, path = %path.display(), "load_chains_from_disk: parse failed");
            return PersistedChains::default();
        }
    };
    let migrated = migrate_v1_value(raw);
    match serde_json::from_value(migrated) {
        Ok(p) => p,
        Err(e) => {
            warn!(error = %e, path = %path.display(), "load_chains_from_disk: typed parse failed");
            PersistedChains::default()
        }
    }
}

pub async fn save_chains_to_disk(state: &EffectsState) {
    if state
        .suppress_save
        .load(std::sync::atomic::Ordering::Relaxed)
    {
        tracing::debug!("save_chains_to_disk: suppressed (apply_persisted_chains in flight)");
        return;
    }
    let mut snapshot = state.snapshot_persisted().await;
    snapshot.version = CURRENT_VERSION;
    let path = chains_path();
    let bytes = match serde_json::to_vec_pretty(&snapshot) {
        Ok(b) => b,
        Err(e) => {
            warn!(error = %e, "save_chains_to_disk: serialize failed");
            return;
        }
    };
    let tmp = path.with_extension("json.tmp");
    if let Err(e) = std::fs::write(&tmp, &bytes) {
        warn!(error = %e, path = %tmp.display(), "save_chains_to_disk: write failed");
        return;
    }
    if let Err(e) = std::fs::rename(&tmp, &path) {
        warn!(error = %e, path = %path.display(), "save_chains_to_disk: rename failed");
    }
}

/// Pull every loaded plugin's current state via `Plugin::save_state`, store
/// the resulting blob on each `Effect`, then write `chains.json` AND push the
/// fresh state_b64 into `AppConfig.plugin_data` via `channel_attach_data`.
/// Both sinks are required because the first-call sync in
/// `pipewire_contributor::respond` reconciles engine state from AppConfig —
/// if AppConfig only sees blobs from when an effect was first added, every
/// restart wipes any param tweaks the user made since.
pub async fn refresh_state_and_save(state: Arc<EffectsState>) {
    if state
        .suppress_save
        .load(std::sync::atomic::Ordering::Relaxed)
    {
        tracing::debug!("refresh_state_and_save: suppressed");
        return;
    }
    if let Some(engine) = state.engine() {
        let snapshot = engine.snapshot_all_states();
        if !snapshot.is_empty() {
            let mut effects = state.effects.lock().await;
            for ((channel_id, effect_id), blob) in snapshot {
                if let Some(slot) = effects.get_mut(&(channel_id, effect_id)) {
                    slot.effect.state_b64 = Some(STANDARD.encode(&blob));
                }
            }
        }
    }
    let in_mem: usize = state
        .effects
        .lock()
        .await
        .len();
    let on_disk: usize = load_chains_from_disk()
        .channels
        .values()
        .map(|c| c.effects.len())
        .sum();
    if in_mem == 0 && on_disk > 0 {
        tracing::warn!(
            on_disk,
            "refresh_state_and_save: in-memory state empty but disk has effects — skipping save to avoid wiping user data"
        );
        return;
    }
    save_chains_to_disk(&state).await;

    // Mirror the same blobs into the host's AppConfig.plugin_data so the
    // first-call reconcile on next launch sees the latest values, not the
    // initial state_b64 captured when the effect was first added.
    if let Some(host) = state.host.get() {
        for channel in state.channels_with_effects().await {
            if let Err(e) = crate::iframe_bridge::persist_channel(&state, host, channel).await {
                tracing::warn!(?e, %channel, "refresh_state_and_save: persist_channel failed");
            }
        }
    }
}

pub fn encode_state(bytes: &[u8]) -> String {
    STANDARD.encode(bytes)
}

pub fn decode_state(s: &str) -> Option<Vec<u8>> {
    STANDARD.decode(s).ok()
}

pub async fn save_effect_state(
    state: Arc<EffectsState>,
    channel_id: Uuid,
    effect_id: Uuid,
) -> anyhow::Result<String> {
    let engine = state
        .engine()
        .ok_or_else(|| anyhow::anyhow!("engine not initialized"))?;
    let blob = engine.save_slot_state(channel_id, effect_id)?;
    Ok(STANDARD.encode(blob))
}
