use serde_json::Value;
use tideline_core::model::AppConfig;
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum MixRpcError {
    #[error("permission denied: namespace {0} is not owned by caller")]
    Forbidden(String),
    #[error("mix uuid {0} not found")]
    NotFound(String),
    #[error("invalid uuid: {0}")]
    BadUuid(String),
}

pub fn handle_attach_data(
    caller_plugin_id: &str,
    mix_uuid: &str,
    namespace: &str,
    value: Value,
    cfg: &mut AppConfig,
) -> Result<(), MixRpcError> {
    if caller_plugin_id != namespace {
        return Err(MixRpcError::Forbidden(namespace.into()));
    }
    let uuid = Uuid::parse_str(mix_uuid).map_err(|_| MixRpcError::BadUuid(mix_uuid.into()))?;
    let m = cfg.mix_by_uuid_mut(uuid).ok_or_else(|| MixRpcError::NotFound(mix_uuid.into()))?;
    m.plugin_data.insert(namespace.into(), value);
    Ok(())
}

pub fn handle_detach_data(
    caller_plugin_id: &str,
    mix_uuid: &str,
    namespace: &str,
    cfg: &mut AppConfig,
) -> Result<(), MixRpcError> {
    if caller_plugin_id != namespace {
        return Err(MixRpcError::Forbidden(namespace.into()));
    }
    let uuid = Uuid::parse_str(mix_uuid).map_err(|_| MixRpcError::BadUuid(mix_uuid.into()))?;
    let m = cfg.mix_by_uuid_mut(uuid).ok_or_else(|| MixRpcError::NotFound(mix_uuid.into()))?;
    m.plugin_data.remove(namespace);
    Ok(())
}
