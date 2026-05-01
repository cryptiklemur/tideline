use std::sync::Arc;

use tideline_host::PluginRegistry;

pub async fn dispatch_plugin_keybind(
    registry: &Arc<PluginRegistry>,
    plugin_id: &str,
    action_id: &str,
) -> Result<(), String> {
    registry
        .dispatch_event(
            plugin_id,
            "keybind/invoke",
            serde_json::json!({ "action_id": action_id }),
        )
        .await
        .map_err(|e| e.to_string())
}
