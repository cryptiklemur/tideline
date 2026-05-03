//! Pipewire contribute_request handler. Filled in by later tasks.
use std::sync::Arc;

use serde_json::Value;
use tideline_sdk::rpc::RpcError;

use crate::state::EffectsState;

pub async fn respond(
    _state: &Arc<EffectsState>,
    _params: Option<Value>,
) -> Result<Value, RpcError> {
    unimplemented!("filled in by later tasks")
}
