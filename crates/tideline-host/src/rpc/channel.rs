use serde_json::Value;
use tideline_core::model::AppConfig;
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum ChannelRpcError {
    #[error("permission denied: namespace {0} is not owned by caller")]
    Forbidden(String),
    #[error("channel uuid {0} not found")]
    NotFound(String),
    #[error("invalid uuid: {0}")]
    BadUuid(String),
}

pub fn handle_attach_data(
    caller_plugin_id: &str,
    channel_uuid: &str,
    namespace: &str,
    value: Value,
    cfg: &mut AppConfig,
) -> Result<(), ChannelRpcError> {
    if caller_plugin_id != namespace {
        return Err(ChannelRpcError::Forbidden(namespace.into()));
    }
    let uuid =
        Uuid::parse_str(channel_uuid).map_err(|_| ChannelRpcError::BadUuid(channel_uuid.into()))?;
    let ch = cfg
        .channel_by_uuid_mut(uuid)
        .ok_or_else(|| ChannelRpcError::NotFound(channel_uuid.into()))?;
    ch.plugin_data.insert(namespace.into(), value);
    Ok(())
}

pub fn handle_detach_data(
    caller_plugin_id: &str,
    channel_uuid: &str,
    namespace: &str,
    cfg: &mut AppConfig,
) -> Result<(), ChannelRpcError> {
    if caller_plugin_id != namespace {
        return Err(ChannelRpcError::Forbidden(namespace.into()));
    }
    let uuid =
        Uuid::parse_str(channel_uuid).map_err(|_| ChannelRpcError::BadUuid(channel_uuid.into()))?;
    let ch = cfg
        .channel_by_uuid_mut(uuid)
        .ok_or_else(|| ChannelRpcError::NotFound(channel_uuid.into()))?;
    ch.plugin_data.remove(namespace);
    Ok(())
}
