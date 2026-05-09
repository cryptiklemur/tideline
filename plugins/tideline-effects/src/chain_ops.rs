//! Per-channel chain operations against the in-process audio engine.
use std::sync::Arc;

use anyhow::Result;
use base64::Engine as _;
use uuid::Uuid;

use crate::effect::Effect;
use crate::host::PluginInfo;
use crate::state::EffectsState;

pub(crate) fn slug_for_channel(channel_id: Uuid) -> String {
    channel_id.simple().to_string()
}

pub(crate) fn decode_state(b64: &Option<String>) -> Option<Vec<u8>> {
    let s = b64.as_ref()?;
    base64::engine::general_purpose::STANDARD.decode(s).ok()
}

pub(crate) fn plugin_info_for(state: &EffectsState, effect: &Effect) -> Result<PluginInfo> {
    state
        .find_catalog_entry(effect.format, &effect.uri)
        .ok_or_else(|| anyhow::anyhow!("plugin {} not in catalog", effect.uri))
}

pub async fn add_effect(state: Arc<EffectsState>, channel_id: Uuid, effect: Effect) -> Result<()> {
    let engine = state
        .engine()
        .ok_or_else(|| anyhow::anyhow!("engine not initialized"))?;
    let info = plugin_info_for(&state, &effect)?;
    let slug = slug_for_channel(channel_id);
    engine.ensure_channel(channel_id, &slug)?;
    let state_blob = decode_state(&effect.state_b64);
    engine.add_plugin(channel_id, effect.id, &info, state_blob.as_deref())?;
    state.attach_effect(channel_id, effect).await;
    Ok(())
}

pub async fn remove_effect(
    state: Arc<EffectsState>,
    channel_id: Uuid,
    effect_id: Uuid,
) -> Result<()> {
    let engine = state
        .engine()
        .ok_or_else(|| anyhow::anyhow!("engine not initialized"))?;
    engine.remove_plugin(channel_id, effect_id)?;
    state.detach_effect(channel_id, effect_id).await;
    Ok(())
}

pub async fn reorder_chain(
    state: Arc<EffectsState>,
    channel_id: Uuid,
    new_order: Vec<Uuid>,
) -> Result<()> {
    let engine = state
        .engine()
        .ok_or_else(|| anyhow::anyhow!("engine not initialized"))?;
    engine.reorder_chain(channel_id, &new_order)?;
    state.set_chain_order(channel_id, new_order).await;
    Ok(())
}
