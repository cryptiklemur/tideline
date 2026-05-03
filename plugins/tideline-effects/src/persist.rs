use std::path::PathBuf;
use std::sync::Arc;

use base64::engine::general_purpose::STANDARD;
use base64::Engine;

use crate::carla::CarlaError;
use crate::state::EffectsState;

pub fn encode_state(bytes: &[u8]) -> String {
    STANDARD.encode(bytes)
}

pub fn decode_state(s: &str) -> Option<Vec<u8>> {
    STANDARD.decode(s).ok()
}

pub async fn save_effect_state(
    state: Arc<EffectsState>,
    channel_id: uuid::Uuid,
    effect_id: uuid::Uuid,
) -> Result<String, CarlaError> {
    let plugin_id = state
        .lookup_plugin_id(channel_id, effect_id)
        .await
        .ok_or_else(|| CarlaError::Ffi(format!("effect {effect_id} not loaded")))?;
    let engine = state
        .engine()
        .await
        .ok_or_else(|| CarlaError::Ffi("engine not initialized".into()))?;
    let dir = tempfile::tempdir().map_err(|e| CarlaError::Ffi(e.to_string()))?;
    let path: PathBuf = dir.path().join("state.xml");
    let host = engine.lock().await;
    host.save_state(plugin_id, &path)?;
    drop(host);
    let bytes = std::fs::read(&path).map_err(|e| CarlaError::Ffi(e.to_string()))?;
    Ok(STANDARD.encode(bytes))
}

pub async fn load_effect_state(
    state: Arc<EffectsState>,
    plugin_id: u32,
    state_b64: &str,
) -> Result<(), CarlaError> {
    let bytes = STANDARD
        .decode(state_b64)
        .map_err(|e| CarlaError::Ffi(e.to_string()))?;
    let dir = tempfile::tempdir().map_err(|e| CarlaError::Ffi(e.to_string()))?;
    let path = dir.path().join("state.xml");
    std::fs::write(&path, bytes).map_err(|e| CarlaError::Ffi(e.to_string()))?;
    let engine = state
        .engine()
        .await
        .ok_or_else(|| CarlaError::Ffi("engine not initialized".into()))?;
    let host = engine.lock().await;
    host.load_state(plugin_id, &path)
}
