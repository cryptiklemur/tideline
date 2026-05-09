//! Typed message bridge between the rack iframe webview and the effects plugin.
//!
//! Inbound messages arrive via the `ui.iframe.message` JSON-RPC method as an
//! [`Envelope`] payload. They are routed by `kind` to chain_ops + AudioEngine.
//! Outbound payloads go back to the iframe via `host.ui_iframe_send`.

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use uuid::Uuid;

use tideline_sdk::rpc::{error_codes, RpcError};
use tideline_sdk::HostClient;

use crate::effect::{ChannelEffectsData, Effect, PluginFormat};
use crate::host::PluginInfo;
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
            send_to_iframe(
                host,
                surface_id,
                OutboundMsg::Hello {
                    plugin_version: env!("CARGO_PKG_VERSION").into(),
                },
            )
            .await?;
            let snap = snapshot_channel(state, channel_uuid).await;
            send_to_iframe(
                host,
                surface_id,
                OutboundMsg::ChannelEffects {
                    channel_uuid,
                    data: snap,
                },
            )
            .await
        }
        InboundMsg::ListPlugins => {
            let plugins = state.catalog_clone().await;
            send_to_iframe(host, surface_id, OutboundMsg::PluginList { plugins }).await
        }
        InboundMsg::AddEffect {
            channel_uuid,
            plugin_uri,
            format,
            display_name,
        } => {
            let effect = Effect {
                id: Uuid::new_v4(),
                format,
                uri: plugin_uri,
                display_name,
                bypassed: false,
                state_b64: None,
            };
            crate::chain_ops::add_effect(state.clone(), channel_uuid, effect)
                .await
                .map_err(|e| internal(format!("add_effect: {e}")))?;
            persist_channel(state, host, channel_uuid).await
        }
        InboundMsg::RemoveEffect {
            channel_uuid,
            effect_id,
        } => {
            crate::chain_ops::remove_effect(state.clone(), channel_uuid, effect_id)
                .await
                .map_err(|e| internal(format!("remove_effect: {e}")))?;
            persist_channel(state, host, channel_uuid).await
        }
        InboundMsg::ToggleBypass {
            channel_uuid,
            effect_id,
            bypassed,
        } => {
            state
                .set_effect_bypassed(channel_uuid, effect_id, bypassed)
                .await;
            persist_channel(state, host, channel_uuid).await
        }
        InboundMsg::ToggleChainBypass {
            channel_uuid,
            bypassed,
        } => {
            state.set_chain_bypass(channel_uuid, bypassed).await;
            persist_channel(state, host, channel_uuid).await
        }
        InboundMsg::Reorder {
            channel_uuid,
            new_order,
        } => {
            crate::chain_ops::reorder_chain(state.clone(), channel_uuid, new_order)
                .await
                .map_err(|e| internal(format!("reorder: {e}")))?;
            persist_channel(state, host, channel_uuid).await
        }
        InboundMsg::OpenPluginGui {
            channel_uuid: _,
            effect_id: _,
        } => {
            // Wired in Step 14 once ui_bridge + suil are available.
            Err(internal("plugin GUI not yet implemented".into()))
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
        InboundMsg::DismissBanner => Ok(()),
    }
}

pub(crate) async fn snapshot_channel(
    state: &Arc<EffectsState>,
    channel: Uuid,
) -> ChannelEffectsData {
    let order = state.chain_order(channel).await;
    let chain_bypassed = state.get_chain_bypass(channel).await;
    let lowcut = state.get_lowcut(channel).await;
    let clipguard = state.get_clipguard(channel).await;
    let input_gain = state.get_input_gain(channel).await;
    let effects_map = state.effects.lock().await;
    let mut effects = Vec::with_capacity(order.len());
    for eid in &order {
        if let Some(slot) = effects_map.get(&(channel, *eid)) {
            effects.push(slot.effect.clone());
        }
    }
    ChannelEffectsData {
        effects,
        chain_bypassed,
        lowcut,
        clipguard,
        input_gain,
    }
}

pub(crate) async fn persist_channel(
    state: &Arc<EffectsState>,
    host: &Arc<HostClient>,
    channel: Uuid,
) -> Result<(), RpcError> {
    let snap = snapshot_channel(state, channel).await;
    let value =
        serde_json::to_value(&snap).map_err(|e| internal(format!("serialize snapshot: {e}")))?;
    host.channel_attach_data(channel, value)
        .await
        .map_err(|e| internal(format!("channel.attach_data: {e}")))
}

async fn send_to_iframe(
    host: &Arc<HostClient>,
    surface_id: &str,
    msg: OutboundMsg,
) -> Result<(), RpcError> {
    let value =
        serde_json::to_value(&msg).map_err(|e| internal(format!("serialize outbound: {e}")))?;
    host.ui_iframe_send(surface_id, value)
        .await
        .map_err(|e| internal(format!("ui_iframe_send: {e}")))
}

fn internal(message: String) -> RpcError {
    RpcError {
        code: error_codes::INTERNAL_ERROR,
        message,
        data: None,
    }
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
            InboundMsg::AddEffect {
                channel_uuid,
                plugin_uri,
                format,
                display_name,
            } => {
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
        assert_eq!(
            v.get("kind").and_then(|x| x.as_str()),
            Some("channel_effects")
        );
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
    }

    #[tokio::test]
    async fn snapshot_channel_returns_effects_in_chain_order() {
        let state = EffectsState::new("test");
        let channel = Uuid::new_v4();
        let e1 = Effect::new_lv2("uri-a");
        let e2 = Effect::new_lv2("uri-b");
        let id1 = e1.id;
        let id2 = e2.id;
        state.attach_effect(channel, e1).await;
        state.attach_effect(channel, e2).await;
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
        state.attach_effect(channel, Effect::new_lv2("uri")).await;
        state.set_chain_bypass(channel, true).await;
        let snap = snapshot_channel(&state, channel).await;
        assert!(snap.chain_bypassed);
    }
}
