use serde::{Deserialize, Serialize};
use serde_json::Value;
use tideline_core::pipewire::directive::PipewireDirective;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SerializedAppConfig {
    pub json: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipewireContributeRequest {
    pub config: SerializedAppConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipewireContributeResponse {
    pub directives: Vec<PipewireDirective>,
}
