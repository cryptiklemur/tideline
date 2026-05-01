use serde_json::Value;
use std::collections::HashMap;

#[derive(Debug, Default)]
pub struct NamespaceStore {
    inner: HashMap<String, Value>,
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigRpcError {
    #[error("permission denied: cannot write to namespace {0}")]
    Forbidden(String),
}

pub fn handle_namespace_get(
    _caller_plugin_id: &str,
    namespace: &str,
    store: &NamespaceStore,
) -> Result<Value, ConfigRpcError> {
    Ok(store.inner.get(namespace).cloned().unwrap_or(Value::Null))
}

pub fn handle_namespace_set(
    caller_plugin_id: &str,
    namespace: &str,
    value: Value,
    store: &mut NamespaceStore,
) -> Result<(), ConfigRpcError> {
    if caller_plugin_id != namespace {
        return Err(ConfigRpcError::Forbidden(namespace.into()));
    }
    store.inner.insert(namespace.into(), value);
    Ok(())
}
