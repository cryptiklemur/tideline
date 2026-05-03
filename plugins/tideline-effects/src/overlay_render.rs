//! Settings + channel overlay render/event dispatch. Filled in by later tasks.
use std::sync::Arc;

use serde_json::Value;
use tideline_sdk::rpc::RpcError;
use tideline_sdk::HostClient;

use crate::state::EffectsState;

pub async fn render_settings(
    _state: &Arc<EffectsState>,
    _params: Option<Value>,
) -> Result<Value, RpcError> {
    unimplemented!("filled in by later tasks")
}

pub async fn handle_settings_event(
    _state: &Arc<EffectsState>,
    _host: Arc<HostClient>,
    _params: Option<Value>,
) -> Result<Value, RpcError> {
    unimplemented!("filled in by later tasks")
}

pub async fn render_overlay(
    _state: &Arc<EffectsState>,
    _params: Option<Value>,
) -> Result<Value, RpcError> {
    unimplemented!("filled in by later tasks")
}

pub async fn handle_overlay_event(
    _state: &Arc<EffectsState>,
    _host: Arc<HostClient>,
    _params: Option<Value>,
) -> Result<Value, RpcError> {
    unimplemented!("filled in by later tasks")
}
