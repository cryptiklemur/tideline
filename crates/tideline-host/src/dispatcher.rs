use crate::backend::HostBackend;
use crate::capabilities::{required_capability_for, CapabilitySet};
use crate::events::EventBus;
use crate::registry::{ContribKind, PluginRegistry};
use serde_json::{json, Value};
use std::sync::{Arc, Weak};
use tideline_sdk::rpc::{error_codes, RpcError};
use tokio::sync::RwLock;

#[derive(Clone)]
pub struct HostContext {
    pub plugin_id: String,
    pub granted: Arc<RwLock<CapabilitySet>>,
    pub bus: EventBus,
    pub backend: Arc<dyn HostBackend>,
    /// Weak ref so the dispatcher can call back into the registry to mutate
    /// per-plugin contribution storage. Weak avoids the Arc cycle that would
    /// otherwise leak the registry once the dispatcher loop is spawned.
    pub registry: Weak<PluginRegistry>,
}

async fn register_contrib(
    ctx: &HostContext,
    kind: ContribKind,
    params: Option<Value>,
) -> Result<Value, RpcError> {
    let payload = params.unwrap_or(json!({}));
    let registry = ctx.registry.upgrade().ok_or_else(|| RpcError {
        code: error_codes::INTERNAL_ERROR,
        message: "registry has been dropped".into(),
        data: None,
    })?;
    registry
        .register_contribution(&ctx.plugin_id, kind, payload)
        .await
        .map_err(|e| RpcError {
            code: error_codes::INVALID_PARAMS,
            message: e.to_string(),
            data: None,
        })?;
    Ok(json!({}))
}

async fn unregister_contrib(
    ctx: &HostContext,
    kind: ContribKind,
    params: Option<Value>,
    id_field: &str,
) -> Result<Value, RpcError> {
    let p = params.unwrap_or(json!({}));
    let id = p
        .get(id_field)
        .and_then(|v| v.as_str())
        .ok_or_else(|| RpcError {
            code: error_codes::INVALID_PARAMS,
            message: format!("missing {id_field} string"),
            data: None,
        })?;
    let registry = ctx.registry.upgrade().ok_or_else(|| RpcError {
        code: error_codes::INTERNAL_ERROR,
        message: "registry has been dropped".into(),
        data: None,
    })?;
    registry
        .unregister_contribution(&ctx.plugin_id, kind, id)
        .await
        .map_err(|e| RpcError {
            code: error_codes::INTERNAL_ERROR,
            message: e.to_string(),
            data: None,
        })?;
    Ok(json!({}))
}

pub async fn dispatch(
    ctx: &HostContext,
    method: &str,
    params: Option<Value>,
) -> Result<Value, RpcError> {
    if let Some(required) = required_capability_for(method) {
        if !ctx.granted.read().await.has(required) {
            return Err(RpcError {
                code: error_codes::CAPABILITY_DENIED,
                message: format!("plugin {:?} lacks capability for {method}", ctx.plugin_id),
                data: Some(json!({"required": required})),
            });
        }
    }
    match method {
        "host/initialize" => Ok(json!({
            "host_version": env!("CARGO_PKG_VERSION"),
            "api": "1.x",
        })),
        "host/log.write" => {
            let p = params.unwrap_or(json!({}));
            let level = p
                .get("level")
                .and_then(|v| v.as_str())
                .unwrap_or("info")
                .to_string();
            let message = p
                .get("message")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            tracing::info!(plugin = %ctx.plugin_id, level = %level, "{message}");
            Ok(json!({}))
        }
        "host/event.subscribe" => {
            let topic = params
                .as_ref()
                .and_then(|v| v.get("topic").and_then(|t| t.as_str()))
                .unwrap_or("")
                .to_string();
            ctx.bus
                .subscribe(&ctx.plugin_id, &topic)
                .await
                .map_err(|e| RpcError {
                    code: error_codes::INTERNAL_ERROR,
                    message: e.to_string(),
                    data: None,
                })?;
            Ok(json!({}))
        }
        "host/event.unsubscribe" => {
            let topic = params
                .as_ref()
                .and_then(|v| v.get("topic").and_then(|t| t.as_str()))
                .unwrap_or("")
                .to_string();
            ctx.bus
                .unsubscribe(&ctx.plugin_id, &topic)
                .await
                .map_err(|e| RpcError {
                    code: error_codes::INTERNAL_ERROR,
                    message: e.to_string(),
                    data: None,
                })?;
            Ok(json!({}))
        }
        "host/event.publish" => {
            let p = params.unwrap_or(json!({}));
            let topic = p
                .get("topic")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let payload = p.get("params").cloned().unwrap_or(json!({}));
            ctx.bus
                .publish_plugin(&ctx.plugin_id, &topic, payload)
                .await
                .map_err(|e| RpcError {
                    code: error_codes::TOPIC_UNDECLARED,
                    message: e.to_string(),
                    data: None,
                })?;
            Ok(json!({}))
        }
        "host/channel.list" => Ok(json!([])),
        "host/channel.get" => Ok(json!(null)),
        "host/channel.subscribe_meters" => Ok(json!({})),
        "host/channel.create" => Ok(json!({"id": format!("ch-{}", uuid::Uuid::new_v4())})),
        "host/channel.update" => Ok(json!({})),
        "host/channel.attach_data" => {
            let p = params.unwrap_or(json!({}));
            let channel_uuid: uuid::Uuid = p
                .get("channel_uuid")
                .and_then(|v| v.as_str())
                .and_then(|s| s.parse().ok())
                .ok_or_else(|| RpcError {
                    code: error_codes::INVALID_PARAMS,
                    message: "channel.attach_data: missing/invalid channel_uuid".into(),
                    data: None,
                })?;
            let data = p.get("data").cloned().unwrap_or(json!(null));
            ctx.backend
                .attach_channel_data(&ctx.plugin_id, channel_uuid, data)
                .await
                .map_err(|e| RpcError {
                    code: error_codes::INTERNAL_ERROR,
                    message: e,
                    data: None,
                })?;
            Ok(json!({}))
        }
        "host/mix.attach_data" => Ok(json!({})),
        "host/levels.read" => Ok(json!({"channels": []})),
        "host/audio.play" => Ok(json!({})),
        "host/source.set_mute" => {
            let p = params.unwrap_or(json!({}));
            let node = p
                .get("node")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let muted = p.get("muted").and_then(|v| v.as_bool()).unwrap_or(false);
            ctx.backend
                .set_source_mute(&node, muted)
                .await
                .map_err(|e| RpcError {
                    code: error_codes::INTERNAL_ERROR,
                    message: e,
                    data: None,
                })?;
            Ok(json!({}))
        }
        "host/sources.list" => {
            let sources = ctx
                .backend
                .list_input_sources()
                .await
                .map_err(|e| RpcError {
                    code: error_codes::INTERNAL_ERROR,
                    message: e,
                    data: None,
                })?;
            Ok(json!({"sources": sources}))
        }
        "host/audio.position" => Ok(json!({})),
        "host/notify" | "host/notify.send" => {
            let p = params.unwrap_or(json!({}));
            let title = p
                .get("title")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let body = p
                .get("body")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            ctx.backend
                .notify(&title, &body)
                .await
                .map_err(|e| RpcError {
                    code: error_codes::INTERNAL_ERROR,
                    message: e,
                    data: None,
                })?;
            Ok(json!({}))
        }
        "plugin/settings.section.render" => {
            tracing::info!(
                "dispatcher: plugin/settings.section.render called, params={:?}",
                params
            );
            let p = params.unwrap_or(json!({}));
            let surface_id = p
                .get("surface_id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| RpcError {
                    code: error_codes::INVALID_PARAMS,
                    message: "missing surface_id".into(),
                    data: None,
                })?
                .to_string();
            let tree = p.get("tree").cloned().unwrap_or(serde_json::Value::Null);
            let registry = ctx.registry.upgrade().ok_or_else(|| RpcError {
                code: error_codes::INTERNAL_ERROR,
                message: "registry dropped".into(),
                data: None,
            })?;
            registry
                .update_settings_section_tree(&ctx.plugin_id, &surface_id, tree)
                .await
                .map_err(|e| RpcError {
                    code: error_codes::INTERNAL_ERROR,
                    message: e.to_string(),
                    data: None,
                })?;
            Ok(json!({}))
        }
        "host/ui.iframe.show" | "host/ui.iframe.hide" | "host/ui.iframe.send" => Ok(json!({})),
        "host/ui.channel_overlay.focus" => Ok(json!({})),
        "host/keybind.register" | "host/keybind.unregister" => Ok(json!({})),
        "host/contributions.register_settings_section" => {
            register_contrib(ctx, ContribKind::SettingsSection, params).await
        }
        "host/contributions.unregister_settings_section" => {
            unregister_contrib(ctx, ContribKind::SettingsSection, params, "surface_id").await
        }
        "host/contributions.register_status_pill" => {
            register_contrib(ctx, ContribKind::StatusPill, params).await
        }
        "host/contributions.unregister_status_pill" => {
            unregister_contrib(ctx, ContribKind::StatusPill, params, "surface_id").await
        }
        "host/contributions.register_channel_overlay" => {
            register_contrib(ctx, ContribKind::ChannelOverlay, params).await
        }
        "host/contributions.unregister_channel_overlay" => {
            unregister_contrib(ctx, ContribKind::ChannelOverlay, params, "surface_id").await
        }
        "host/contributions.register_iframe_surface" => {
            register_contrib(ctx, ContribKind::IframeSurface, params).await
        }
        "host/contributions.unregister_iframe_surface" => {
            unregister_contrib(ctx, ContribKind::IframeSurface, params, "surface_id").await
        }
        "host/contributions.register_tray_item" => {
            register_contrib(ctx, ContribKind::TrayItem, params).await
        }
        "host/contributions.unregister_tray_item" => {
            unregister_contrib(ctx, ContribKind::TrayItem, params, "item_id").await
        }
        "host/contributions.register_keybind_action" => {
            register_contrib(ctx, ContribKind::KeybindAction, params).await
        }
        "host/contributions.unregister_keybind_action" => {
            unregister_contrib(ctx, ContribKind::KeybindAction, params, "action_id").await
        }
        "host/contributions.register_input_overlay" => {
            register_contrib(ctx, ContribKind::InputOverlay, params).await
        }
        "host/contributions.unregister_input_overlay" => {
            unregister_contrib(ctx, ContribKind::InputOverlay, params, "surface_id").await
        }
        "host/pipewire.contribute" => Ok(json!({})),
        "host/config.namespace.get" => {
            let p = params.unwrap_or(json!({}));
            let namespace = p
                .get("namespace")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let value = ctx
                .backend
                .config_namespace_get(&namespace)
                .await
                .map_err(|e| RpcError {
                    code: error_codes::INTERNAL_ERROR,
                    message: e,
                    data: None,
                })?;
            Ok(value)
        }
        "host/config.namespace.set" => {
            let p = params.unwrap_or(json!({}));
            let namespace = p
                .get("namespace")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let value = p.get("value").cloned().unwrap_or(json!(null));
            ctx.backend
                .config_namespace_set(&namespace, value)
                .await
                .map_err(|e| RpcError {
                    code: error_codes::INTERNAL_ERROR,
                    message: e,
                    data: None,
                })?;
            Ok(json!({}))
        }
        "host/config.read" => Ok(json!({})),
        "host/config.write" => Ok(json!({})),
        "host/fs.read" | "host/fs.write" => Ok(json!({})),
        "host/net.http" => Ok(json!({"status": 200, "body": ""})),
        "host/process.spawn" => Ok(json!({"pid": 0})),
        "host/secrets.read" => Ok(json!({"value": null})),
        "host/secrets.write" => Ok(json!({})),
        _ => Err(RpcError {
            code: error_codes::METHOD_NOT_FOUND,
            message: format!("unknown host method {method}"),
            data: None,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tideline_sdk::Capability::*;

    #[tokio::test]
    async fn capability_denied_when_not_granted() {
        let ctx = HostContext {
            plugin_id: "p".into(),
            granted: Arc::new(RwLock::new(CapabilitySet::default())),
            bus: EventBus::new(),
            backend: crate::backend::null_backend(),
            registry: Weak::new(),
        };
        let err = dispatch(&ctx, "host/log.write", Some(json!({})))
            .await
            .unwrap_err();
        assert_eq!(err.code, error_codes::CAPABILITY_DENIED);
    }

    #[tokio::test]
    async fn capability_granted_passes() {
        let ctx = HostContext {
            plugin_id: "p".into(),
            granted: Arc::new(RwLock::new(CapabilitySet::new([LogWrite]))),
            bus: EventBus::new(),
            backend: crate::backend::null_backend(),
            registry: Weak::new(),
        };
        dispatch(
            &ctx,
            "host/log.write",
            Some(json!({"level":"info","message":"ok"})),
        )
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn initialize_does_not_require_capability() {
        let ctx = HostContext {
            plugin_id: "p".into(),
            granted: Arc::new(RwLock::new(CapabilitySet::default())),
            bus: EventBus::new(),
            backend: crate::backend::null_backend(),
            registry: Weak::new(),
        };
        dispatch(&ctx, "host/initialize", None).await.unwrap();
    }

    #[tokio::test]
    async fn set_mute_routes_to_backend() {
        use async_trait::async_trait;
        use std::sync::Mutex;

        struct RecordingBackend {
            calls: Mutex<Vec<(String, bool)>>,
        }
        #[async_trait]
        impl crate::backend::HostBackend for RecordingBackend {
            async fn set_source_mute(&self, node: &str, muted: bool) -> Result<(), String> {
                self.calls.lock().unwrap().push((node.to_string(), muted));
                Ok(())
            }
            async fn notify(&self, _t: &str, _b: &str) -> Result<(), String> {
                Ok(())
            }
            async fn list_input_sources(&self) -> Result<Vec<crate::backend::AudioSource>, String> {
                Ok(Vec::new())
            }
            async fn config_namespace_get(&self, _n: &str) -> Result<serde_json::Value, String> {
                Ok(serde_json::Value::Null)
            }
            async fn config_namespace_set(
                &self,
                _n: &str,
                _v: serde_json::Value,
            ) -> Result<(), String> {
                Ok(())
            }
            async fn attach_channel_data(
                &self,
                _n: &str,
                _c: uuid::Uuid,
                _v: serde_json::Value,
            ) -> Result<(), String> {
                Ok(())
            }
        }

        let backend = Arc::new(RecordingBackend {
            calls: Mutex::new(Vec::new()),
        });
        let ctx = HostContext {
            plugin_id: "p".into(),
            granted: Arc::new(RwLock::new(CapabilitySet::new([AudioMute]))),
            bus: EventBus::new(),
            backend: backend.clone(),
            registry: Weak::new(),
        };
        dispatch(
            &ctx,
            "host/source.set_mute",
            Some(json!({"node": "src", "muted": true})),
        )
        .await
        .unwrap();
        let calls = backend.calls.lock().unwrap().clone();
        assert_eq!(calls, vec![("src".to_string(), true)]);
    }

    #[tokio::test]
    async fn notify_routes_to_backend() {
        use async_trait::async_trait;
        use std::sync::Mutex;

        struct RecordingBackend {
            calls: Mutex<Vec<(String, String)>>,
        }
        #[async_trait]
        impl crate::backend::HostBackend for RecordingBackend {
            async fn set_source_mute(&self, _n: &str, _m: bool) -> Result<(), String> {
                Ok(())
            }
            async fn notify(&self, title: &str, body: &str) -> Result<(), String> {
                self.calls
                    .lock()
                    .unwrap()
                    .push((title.to_string(), body.to_string()));
                Ok(())
            }
            async fn list_input_sources(&self) -> Result<Vec<crate::backend::AudioSource>, String> {
                Ok(Vec::new())
            }
            async fn config_namespace_get(&self, _n: &str) -> Result<serde_json::Value, String> {
                Ok(serde_json::Value::Null)
            }
            async fn config_namespace_set(
                &self,
                _n: &str,
                _v: serde_json::Value,
            ) -> Result<(), String> {
                Ok(())
            }
            async fn attach_channel_data(
                &self,
                _n: &str,
                _c: uuid::Uuid,
                _v: serde_json::Value,
            ) -> Result<(), String> {
                Ok(())
            }
        }

        let backend = Arc::new(RecordingBackend {
            calls: Mutex::new(Vec::new()),
        });
        let ctx = HostContext {
            plugin_id: "p".into(),
            granted: Arc::new(RwLock::new(CapabilitySet::new([TrayContribute]))),
            bus: EventBus::new(),
            backend: backend.clone(),
            registry: Weak::new(),
        };
        dispatch(
            &ctx,
            "host/notify",
            Some(json!({"title": "T", "body": "B"})),
        )
        .await
        .unwrap();
        let calls = backend.calls.lock().unwrap().clone();
        assert_eq!(calls, vec![("T".to_string(), "B".to_string())]);
    }

    fn ctx_with_registry(
        reg: &Arc<PluginRegistry>,
        caps: impl IntoIterator<Item = tideline_sdk::Capability>,
    ) -> HostContext {
        HostContext {
            plugin_id: "io.test.dispatch".into(),
            granted: Arc::new(RwLock::new(CapabilitySet::new(caps))),
            bus: EventBus::new(),
            backend: crate::backend::null_backend(),
            registry: Arc::downgrade(reg),
        }
    }

    #[tokio::test]
    async fn register_status_pill_arm_writes_into_registry() {
        let reg = PluginRegistry::new_for_test();
        let ctx = ctx_with_registry(&reg, [UiStatusPill]);
        dispatch(
            &ctx,
            "host/contributions.register_status_pill",
            Some(json!({
                "surface_id": "main",
                "label": "Hello"
            })),
        )
        .await
        .unwrap();
        let snap = reg.contributions().await;
        assert_eq!(snap.status_pills.len(), 1);
        assert_eq!(snap.status_pills[0].plugin_id, "io.test.dispatch");
        assert_eq!(snap.status_pills[0].label, "Hello");
    }

    #[tokio::test]
    async fn register_arm_denied_without_capability() {
        let reg = PluginRegistry::new_for_test();
        let ctx = ctx_with_registry(&reg, []);
        let err = dispatch(
            &ctx,
            "host/contributions.register_status_pill",
            Some(json!({"surface_id": "x", "label": "X"})),
        )
        .await
        .unwrap_err();
        assert_eq!(err.code, error_codes::CAPABILITY_DENIED);
        assert_eq!(reg.contributions().await.status_pills.len(), 0);
    }

    #[tokio::test]
    async fn unregister_status_pill_arm_removes_from_registry() {
        let reg = PluginRegistry::new_for_test();
        let ctx = ctx_with_registry(&reg, [UiStatusPill]);
        dispatch(
            &ctx,
            "host/contributions.register_status_pill",
            Some(json!({
                "surface_id": "main", "label": "L"
            })),
        )
        .await
        .unwrap();
        dispatch(
            &ctx,
            "host/contributions.unregister_status_pill",
            Some(json!({"surface_id": "main"})),
        )
        .await
        .unwrap();
        assert_eq!(reg.contributions().await.status_pills.len(), 0);
    }

    #[tokio::test]
    async fn unregister_arm_rejects_missing_id_field() {
        let reg = PluginRegistry::new_for_test();
        let ctx = ctx_with_registry(&reg, [UiStatusPill]);
        let err = dispatch(
            &ctx,
            "host/contributions.unregister_status_pill",
            Some(json!({})),
        )
        .await
        .unwrap_err();
        assert_eq!(err.code, error_codes::INVALID_PARAMS);
    }

    #[tokio::test]
    async fn each_register_arm_routes_to_correct_kind() {
        let reg = PluginRegistry::new_for_test();
        let ctx = ctx_with_registry(
            &reg,
            [
                UiSettingsSection,
                UiStatusPill,
                UiChannelOverlay,
                UiIframe,
                TrayContribute,
                KeybindRegister,
                UiInputOverlay,
            ],
        );
        dispatch(
            &ctx,
            "host/contributions.register_settings_section",
            Some(json!({"surface_id": "s", "title": "T", "tree": {}})),
        )
        .await
        .unwrap();
        dispatch(
            &ctx,
            "host/contributions.register_status_pill",
            Some(json!({"surface_id": "p", "label": "L"})),
        )
        .await
        .unwrap();
        dispatch(
            &ctx,
            "host/contributions.register_channel_overlay",
            Some(json!({
                "surface_id": "o",
                "placement": "detail",
                "channel_filter": {"kind": "all"},
                "tree": {}
            })),
        )
        .await
        .unwrap();
        dispatch(
            &ctx,
            "host/contributions.register_iframe_surface",
            Some(json!({"surface_id": "f", "entry_path": "u"})),
        )
        .await
        .unwrap();
        dispatch(
            &ctx,
            "host/contributions.register_tray_item",
            Some(json!({"item_id": "t", "label": "Q"})),
        )
        .await
        .unwrap();
        dispatch(
            &ctx,
            "host/contributions.register_keybind_action",
            Some(json!({"action_id": "a", "label": "X"})),
        )
        .await
        .unwrap();
        dispatch(
            &ctx,
            "host/contributions.register_input_overlay",
            Some(json!({
                "surface_id": "io",
                "input_filter": {"kind": "all"},
                "tree": {}
            })),
        )
        .await
        .unwrap();
        let snap = reg.contributions().await;
        assert_eq!(snap.settings_sections.len(), 1);
        assert_eq!(snap.status_pills.len(), 1);
        assert_eq!(snap.channel_overlays.len(), 1);
        assert_eq!(snap.iframe_surfaces.len(), 1);
        assert_eq!(snap.tray_items.len(), 1);
        assert_eq!(snap.keybind_actions.len(), 1);
        assert_eq!(snap.input_overlays.len(), 1);
    }
}
