//! Typed message bridge between the rack iframe webview and the effects plugin.
//!
//! Inbound messages arrive via the `ui.iframe.message` JSON-RPC method as an
//! [`Envelope`] payload. They are routed by `kind` to the FFI primitives in
//! `chain_ops`, `persist`, `discovery`, and `install*`. Outbound payloads go
//! back to the iframe via `host.ui_iframe_send` (NOT via the response).

use std::panic::AssertUnwindSafe;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use uuid::Uuid;

use tideline_sdk::rpc::{error_codes, RpcError};
use tideline_sdk::HostClient;

use crate::discovery::PluginInfo;
use crate::effect::{ChannelEffectsData, Effect, PluginFormat};
use crate::state::EffectsState;

#[derive(Debug, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum InboundMsg {
    Hello {
        channel_uuid: Uuid,
    },
    ListPlugins,
    AddEffect {
        channel_uuid: Uuid,
        plugin_uri: String,
        format: PluginFormat,
        display_name: String,
    },
    RemoveEffect {
        channel_uuid: Uuid,
        effect_id: Uuid,
    },
    ToggleBypass {
        channel_uuid: Uuid,
        effect_id: Uuid,
        bypassed: bool,
    },
    ToggleChainBypass {
        channel_uuid: Uuid,
        bypassed: bool,
    },
    Reorder {
        channel_uuid: Uuid,
        new_order: Vec<Uuid>,
    },
    OpenPluginGui {
        channel_uuid: Uuid,
        effect_id: Uuid,
    },
    SaveAllState {
        channel_uuid: Uuid,
    },
    InstallProbe,
    InstallRun,
    DismissBanner,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum OutboundMsg {
    Hello {
        plugin_version: String,
    },
    PluginList {
        plugins: Vec<PluginInfo>,
    },
    ChannelEffects {
        channel_uuid: Uuid,
        data: ChannelEffectsData,
    },
    InstallProbe {
        needs_install: bool,
    },
    InstallResult {
        success: bool,
        message: String,
    },
    Toast {
        #[serde(rename = "tone")]
        kind: String,
        title: String,
        body: String,
    },
}

#[derive(Debug, Deserialize)]
struct Envelope {
    surface_id: String,
    #[serde(default)]
    #[allow(dead_code)]
    channel_uuid: Option<Uuid>,
    message: InboundMsg,
}

pub async fn dispatch(
    state: &Arc<EffectsState>,
    host: Arc<HostClient>,
    params: Option<Value>,
) -> Result<Value, RpcError> {
    let raw = params.ok_or_else(|| RpcError {
        code: error_codes::INVALID_PARAMS,
        message: "ui.iframe.message: missing params".into(),
        data: None,
    })?;
    let env: Envelope = serde_json::from_value(raw).map_err(|e| RpcError {
        code: error_codes::INVALID_PARAMS,
        message: format!("ui.iframe.message: parse failed: {e}"),
        data: None,
    })?;
    handle(state, &host, &env.surface_id, env.message).await?;
    Ok(json!({}))
}

async fn handle(
    state: &Arc<EffectsState>,
    host: &Arc<HostClient>,
    surface_id: &str,
    msg: InboundMsg,
) -> Result<(), RpcError> {
    match msg {
        InboundMsg::Hello { channel_uuid } => {
            send_to_iframe(host, surface_id, OutboundMsg::Hello {
                plugin_version: env!("CARGO_PKG_VERSION").into(),
            }).await?;
            let snap = snapshot_channel(state, channel_uuid).await;
            send_to_iframe(host, surface_id, OutboundMsg::ChannelEffects {
                channel_uuid,
                data: snap,
            }).await?;
            let probe = crate::install::InstallProbe::run().await;
            send_to_iframe(host, surface_id, OutboundMsg::InstallProbe {
                needs_install: probe.needs_install(),
            }).await
        }
        InboundMsg::ListPlugins => {
            let cache = crate::discovery::ensure_cached().await;
            send_to_iframe(host, surface_id, OutboundMsg::PluginList {
                plugins: cache.plugins,
            }).await
        }
        InboundMsg::AddEffect { channel_uuid, plugin_uri, format, display_name } => {
            let effect = Effect {
                id: Uuid::new_v4(),
                format,
                uri: plugin_uri,
                display_name,
                bypassed: false,
                state_b64: None,
            };
            crate::chain_ops::add_effect(state.clone(), channel_uuid, effect).await
                .map_err(|e| internal(format!("add_effect: {e}")))?;
            persist_channel(state, host, channel_uuid).await
        }
        InboundMsg::RemoveEffect { channel_uuid, effect_id } => {
            crate::chain_ops::remove_effect(state.clone(), channel_uuid, effect_id).await
                .map_err(|e| internal(format!("remove_effect: {e}")))?;
            persist_channel(state, host, channel_uuid).await
        }
        InboundMsg::ToggleBypass { channel_uuid, effect_id, bypassed } => {
            let plugin_id = state.lookup_plugin_id(channel_uuid, effect_id).await
                .ok_or_else(|| internal(format!("effect {effect_id} not in chain")))?;
            engine_set_active(state, plugin_id, !bypassed).await?;
            state.set_effect_bypassed(channel_uuid, effect_id, bypassed).await;
            persist_channel(state, host, channel_uuid).await
        }
        InboundMsg::ToggleChainBypass { channel_uuid, bypassed } => {
            let order = state.chain_order(channel_uuid).await;
            for eid in &order {
                if let Some(pid) = state.lookup_plugin_id(channel_uuid, *eid).await {
                    engine_set_active(state, pid, !bypassed).await?;
                }
            }
            state.set_chain_bypass(channel_uuid, bypassed).await;
            persist_channel(state, host, channel_uuid).await
        }
        InboundMsg::Reorder { channel_uuid, new_order } => {
            crate::chain_ops::reorder_chain(state.clone(), channel_uuid, new_order).await
                .map_err(|e| internal(format!("reorder: {e}")))?;
            persist_channel(state, host, channel_uuid).await
        }
        InboundMsg::OpenPluginGui { channel_uuid, effect_id } => {
            let plugin_id = state.lookup_plugin_id(channel_uuid, effect_id).await
                .ok_or_else(|| internal(format!("effect {effect_id} not in chain")))?;
            engine_show_custom_ui(state, plugin_id, true).await
        }
        InboundMsg::SaveAllState { channel_uuid } => {
            let order = state.chain_order(channel_uuid).await;
            for eid in order {
                let b64 = crate::persist::save_effect_state(state.clone(), channel_uuid, eid)
                    .await
                    .map_err(|e| internal(format!("save_effect_state({eid}): {e}")))?;
                state.set_effect_state_b64(channel_uuid, eid, b64).await;
            }
            persist_channel(state, host, channel_uuid).await
        }
        InboundMsg::InstallProbe => {
            let probe = crate::install::InstallProbe::run().await;
            send_to_iframe(host, surface_id, OutboundMsg::InstallProbe {
                needs_install: probe.needs_install(),
            }).await
        }
        InboundMsg::InstallRun => {
            match crate::install_runner::run_install_via_pkexec().await {
                Ok(outcome) => {
                    send_to_iframe(host, surface_id, OutboundMsg::InstallResult {
                        success: outcome.success,
                        message: outcome.stderr_tail,
                    }).await
                }
                Err(e) => {
                    send_to_iframe(host, surface_id, OutboundMsg::InstallResult {
                        success: false,
                        message: format!("spawn failed: {e}"),
                    }).await
                }
            }
        }
        InboundMsg::DismissBanner => {
            // TODO(T20): persist install_banner_dismissed via config_namespace_set
            Ok(())
        }
    }
}

async fn engine_set_active(
    state: &Arc<EffectsState>,
    plugin_id: u32,
    on: bool,
) -> Result<(), RpcError> {
    let engine = state.engine().await
        .ok_or_else(|| internal("engine not initialized".into()))?;
    let host = engine.lock().await;
    let result = std::panic::catch_unwind(AssertUnwindSafe(|| host.set_active(plugin_id, on)));
    drop(host);
    if let Err(panic) = result {
        let msg = panic_msg(panic);
        crate::engine::mark_unhealthy(state.clone(), msg.clone()).await;
        return Err(internal(format!("set_active panicked: {msg}")));
    }
    Ok(())
}

async fn engine_show_custom_ui(
    state: &Arc<EffectsState>,
    plugin_id: u32,
    show: bool,
) -> Result<(), RpcError> {
    let engine = state.engine().await
        .ok_or_else(|| internal("engine not initialized".into()))?;
    let host = engine.lock().await;
    let result = std::panic::catch_unwind(AssertUnwindSafe(|| host.show_custom_ui(plugin_id, show)));
    drop(host);
    if let Err(panic) = result {
        let msg = panic_msg(panic);
        crate::engine::mark_unhealthy(state.clone(), msg.clone()).await;
        return Err(internal(format!("show_custom_ui panicked: {msg}")));
    }
    Ok(())
}

async fn snapshot_channel(state: &Arc<EffectsState>, channel: Uuid) -> ChannelEffectsData {
    let order = state.chain_order(channel).await;
    let chain_bypassed = state.get_chain_bypass(channel).await;
    let effects_map = state.effects.lock().await;
    let mut effects = Vec::with_capacity(order.len());
    for eid in &order {
        if let Some(slot) = effects_map.get(&(channel, *eid)) {
            effects.push(slot.effect.clone());
        }
    }
    ChannelEffectsData { effects, chain_bypassed }
}

async fn persist_channel(
    state: &Arc<EffectsState>,
    host: &Arc<HostClient>,
    channel: Uuid,
) -> Result<(), RpcError> {
    let snap = snapshot_channel(state, channel).await;
    let value = serde_json::to_value(&snap)
        .map_err(|e| internal(format!("serialize snapshot: {e}")))?;
    host.channel_attach_data(channel, value).await
        .map_err(|e| internal(format!("channel.attach_data: {e}")))
}

async fn send_to_iframe(
    host: &Arc<HostClient>,
    surface_id: &str,
    msg: OutboundMsg,
) -> Result<(), RpcError> {
    let value = serde_json::to_value(&msg)
        .map_err(|e| internal(format!("serialize outbound: {e}")))?;
    host.ui_iframe_send(surface_id, value).await
        .map_err(|e| internal(format!("ui_iframe_send: {e}")))
}

fn internal(message: String) -> RpcError {
    RpcError { code: error_codes::INTERNAL_ERROR, message, data: None }
}

fn panic_msg(panic: Box<dyn std::any::Any + Send>) -> String {
    panic
        .downcast_ref::<&str>()
        .map(|s| (*s).to_string())
        .or_else(|| panic.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "carla FFI panicked".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inbound_msg_parses_add_effect() {
        let channel = Uuid::new_v4();
        let raw = json!({
            "kind": "add_effect",
            "channel_uuid": channel,
            "plugin_uri": "http://lsp-plug.in/plugins/lv2/gate_mono",
            "format": "lv2",
            "display_name": "LSP Gate",
        });
        let msg: InboundMsg = serde_json::from_value(raw).unwrap();
        match msg {
            InboundMsg::AddEffect { channel_uuid, plugin_uri, format, display_name } => {
                assert_eq!(channel_uuid, channel);
                assert_eq!(plugin_uri, "http://lsp-plug.in/plugins/lv2/gate_mono");
                assert_eq!(format, PluginFormat::Lv2);
                assert_eq!(display_name, "LSP Gate");
            }
            other => panic!("expected AddEffect, got {other:?}"),
        }
    }

    #[test]
    fn outbound_msg_serializes_with_snake_case_kind() {
        let msg = OutboundMsg::ChannelEffects {
            channel_uuid: Uuid::nil(),
            data: ChannelEffectsData::default(),
        };
        let v = serde_json::to_value(&msg).unwrap();
        assert_eq!(v.get("kind").and_then(|x| x.as_str()), Some("channel_effects"));
        assert!(v.get("channel_uuid").is_some());
        assert!(v.get("data").is_some());
    }

    #[test]
    fn envelope_parses_with_optional_channel_uuid() {
        let with = json!({
            "surface_id": "rack",
            "channel_uuid": Uuid::nil(),
            "message": {"kind": "list_plugins"},
        });
        let env: Envelope = serde_json::from_value(with).unwrap();
        assert_eq!(env.surface_id, "rack");
        assert!(env.channel_uuid.is_some());
        assert!(matches!(env.message, InboundMsg::ListPlugins));

        let without = json!({
            "surface_id": "rack",
            "message": {"kind": "list_plugins"},
        });
        let env: Envelope = serde_json::from_value(without).unwrap();
        assert!(env.channel_uuid.is_none());
        assert!(matches!(env.message, InboundMsg::ListPlugins));
    }

    #[test]
    fn inbound_msg_unknown_kind_fails_parse() {
        let raw = json!({"kind": "definitely_not_a_real_kind"});
        let res: Result<InboundMsg, _> = serde_json::from_value(raw);
        assert!(res.is_err());
    }

    #[tokio::test]
    async fn snapshot_channel_returns_effects_in_chain_order() {
        let state = EffectsState::new("test");
        let channel = Uuid::new_v4();
        let e1 = Effect::new_lv2("uri-a");
        let e2 = Effect::new_lv2("uri-b");
        let id1 = e1.id;
        let id2 = e2.id;
        state.attach_effect(channel, e1, 10).await;
        state.attach_effect(channel, e2, 11).await;
        let snap = snapshot_channel(&state, channel).await;
        assert_eq!(snap.effects.len(), 2);
        assert_eq!(snap.effects[0].id, id1);
        assert_eq!(snap.effects[1].id, id2);
        assert!(!snap.chain_bypassed);
    }

    #[tokio::test]
    async fn snapshot_channel_reflects_chain_bypass_flag() {
        let state = EffectsState::new("test");
        let channel = Uuid::new_v4();
        state.attach_effect(channel, Effect::new_lv2("uri"), 0).await;
        state.set_chain_bypass(channel, true).await;
        let snap = snapshot_channel(&state, channel).await;
        assert!(snap.chain_bypassed);
    }
}
