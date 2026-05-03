//! ui.iframe message dispatch. Filled in by later tasks.
use std::sync::Arc;

use serde_json::Value;
use tideline_sdk::rpc::RpcError;
use tideline_sdk::HostClient;

use crate::state::EffectsState;

pub async fn dispatch(
    _state: &Arc<EffectsState>,
    _host: Arc<HostClient>,
    _params: Option<Value>,
) -> Result<Value, RpcError> {
    unimplemented!("filled in by later tasks")
}
