//! Settings + channel overlay render/event dispatch.
//!
//! - `channel_card` slot — declarative row with a single `fx` button rendered on
//!   each matrix channel card. Tone reflects whether the channel has a non-empty
//!   effects chain. Click opens the rack modal on the frontend.
//! - `rack` iframe surface — the rack webview shown inside the modal dialog.
//! - settings section — minimal status section.

use std::sync::Arc;

use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

use tideline_sdk::rpc::{error_codes, RpcError};
use tideline_sdk::HostClient;

use crate::state::EffectsState;

const CHANNEL_CARD_SLOT: &str = "channel_card";
#[allow(dead_code)]
pub(crate) const RACK_IFRAME_SLOT: &str = "rack";
const OPEN_RACK_EVENT: &str = "open-rack";

#[derive(Debug, Deserialize)]
struct OverlayRenderRequest {
    surface_id: String,
    channel_uuid: Uuid,
}

#[derive(Debug, Deserialize)]
struct OverlayEventRequest {
    surface_id: String,
    #[allow(dead_code)]
    channel_uuid: Uuid,
    event_id: String,
    #[serde(default)]
    #[allow(dead_code)]
    value: Value,
}

fn parse_params<T: for<'de> Deserialize<'de>>(
    params: Option<Value>,
    method: &'static str,
) -> Result<T, RpcError> {
    let raw = params.ok_or_else(|| RpcError {
        code: error_codes::INVALID_PARAMS,
        message: format!("{method}: missing params"),
        data: None,
    })?;
    serde_json::from_value(raw).map_err(|e| RpcError {
        code: error_codes::INVALID_PARAMS,
        message: format!("{method}: parse failed: {e}"),
        data: None,
    })
}

pub async fn render_settings(
    _state: &Arc<EffectsState>,
    _params: Option<Value>,
) -> Result<Value, RpcError> {
    Ok(settings_tree())
}

pub async fn handle_settings_event(
    _state: &Arc<EffectsState>,
    _host: Arc<HostClient>,
    _params: Option<Value>,
) -> Result<Value, RpcError> {
    Ok(json!({}))
}

pub async fn render_overlay(
    state: &Arc<EffectsState>,
    params: Option<Value>,
) -> Result<Value, RpcError> {
    let req: OverlayRenderRequest = parse_params(params, "channel_overlay.render")?;
    match req.surface_id.as_str() {
        CHANNEL_CARD_SLOT => {
            let has_fx = !state.chain_order(req.channel_uuid).await.is_empty();
            Ok(channel_card_tree(has_fx))
        }
        other => Err(RpcError {
            code: error_codes::INVALID_PARAMS,
            message: format!("channel_overlay.render: unknown surface_id {other}"),
            data: None,
        }),
    }
}

pub async fn handle_overlay_event(
    _state: &Arc<EffectsState>,
    _host: Arc<HostClient>,
    params: Option<Value>,
) -> Result<Value, RpcError> {
    let evt: OverlayEventRequest = parse_params(params, "channel_overlay.event")?;
    if evt.surface_id == CHANNEL_CARD_SLOT && evt.event_id == OPEN_RACK_EVENT {
        // Frontend opens the rack modal directly on click; no plugin-side action needed.
    }
    Ok(json!({}))
}

pub(crate) fn channel_card_tree(has_fx: bool) -> Value {
    let variant = if has_fx { "primary" } else { "ghost" };
    json!({
        "kind": "row",
        "id": "effects-channel-card-row",
        "gap": 0,
        "align": "center",
        "children": [{
            "kind": "button",
            "id": OPEN_RACK_EVENT,
            "text": "",
            "icon": { "name": "fx" },
            "variant": variant,
        }],
    })
}



pub(crate) fn settings_tree() -> Value {
    json!({
        "kind": "section",
        "title": "Effects",
        "children": [{
            "kind": "label",
            "muted": true,
            "text": "Plugin discovery + install controls land alongside the iframe rack UI.",
        }],
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::effect::Effect;

    #[test]
    fn channel_card_variant_reflects_chain_state() {
        let off = channel_card_tree(false);
        assert_eq!(off["children"][0]["variant"], "ghost");
        let on = channel_card_tree(true);
        assert_eq!(on["children"][0]["variant"], "primary");
    }

    #[test]
    fn channel_card_uses_fx_icon_and_open_rack_id() {
        let t = channel_card_tree(true);
        assert_eq!(t["children"][0]["icon"]["name"], "fx");
        assert_eq!(t["children"][0]["id"], OPEN_RACK_EVENT);
        assert_eq!(t["children"][0]["kind"], "button");
    }

    #[test]
    fn settings_tree_is_section() {
        let t = settings_tree();
        assert_eq!(t["kind"], "section");
    }

    #[tokio::test]
    async fn render_overlay_returns_ghost_for_empty_chain() {
        let state = EffectsState::new("test");
        let chan = Uuid::new_v4();
        let params = Some(json!({"surface_id": CHANNEL_CARD_SLOT, "channel_uuid": chan}));
        let v = render_overlay(&state, params).await.unwrap();
        assert_eq!(v["children"][0]["variant"], "ghost");
    }

    #[tokio::test]
    async fn render_overlay_returns_primary_when_chain_present() {
        let state = EffectsState::new("test");
        let chan = Uuid::new_v4();
        state.attach_effect(chan, Effect::new_lv2("uri"), 0).await;
        let params = Some(json!({"surface_id": CHANNEL_CARD_SLOT, "channel_uuid": chan}));
        let v = render_overlay(&state, params).await.unwrap();
        assert_eq!(v["children"][0]["variant"], "primary");
    }

    #[tokio::test]
    async fn render_overlay_unknown_slot_is_invalid_params() {
        let state = EffectsState::new("test");
        let params = Some(json!({"surface_id": "nope", "channel_uuid": Uuid::nil()}));
        let err = render_overlay(&state, params).await.unwrap_err();
        assert_eq!(err.code, error_codes::INVALID_PARAMS);
    }
}
