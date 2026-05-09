use serde::{Deserialize, Serialize};
use serde_json::Value;
use tideline_core::model::AppConfig;
use tideline_core::pipewire::directive::PipewireDirective;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SerializedAppConfig {
    pub json: Value,
}

// Re-export so plugins can `use tideline_sdk::contribute::MixMuteEntry;`
// without needing a direct tideline-core dep.
pub use tideline_core::pipewire::directive::MixMuteEntry;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipewireContributeRequest {
    pub config: SerializedAppConfig,
    #[serde(default)]
    pub mix_mutes: Vec<MixMuteEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipewireContributeResponse {
    pub directives: Vec<PipewireDirective>,
}

pub fn on_pipewire_contribute<F>(
    req: PipewireContributeRequest,
    f: F,
) -> anyhow::Result<PipewireContributeResponse>
where
    F: FnOnce(AppConfig, Vec<MixMuteEntry>) -> anyhow::Result<Vec<PipewireDirective>>,
{
    let cfg: AppConfig = serde_json::from_value(req.config.json)?;
    let directives = f(cfg, req.mix_mutes)?;
    Ok(PipewireContributeResponse { directives })
}
