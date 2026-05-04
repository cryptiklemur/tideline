//! Settings + channel overlay render/event dispatch.
//!
//! - `sidebar_badge` slot — declarative row with a single `fx` button. Tone reflects
//!   whether the channel has a non-empty effects chain.
//! - `detail` slot — placeholder iframe node. The iframe webview lands in T20-T24.
//! - settings section — minimal status section. Detailed install/discovery UI is
//!   layered in once T25 (first-boot discovery) populates the cache.

use std::sync::Arc;

use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

use tideline_sdk::rpc::{error_codes, RpcError};
use tideline_sdk::HostClient;

use crate::state::EffectsState;

const SIDEBAR_BADGE_SLOT: &str = "sidebar_badge";
const DETAIL_SLOT: &str = "detail";
const OPEN_RACK_EVENT: &str = "open-rack";

#[derive(Debug, Deserialize)]
struct OverlayRenderRequest {
    surface_id: String,
    channel_uuid: Uuid,
}

#[derive(Debug, Deserialize)]
struct OverlayEventRequest {
    surface_id: String,
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
        SIDEBAR_BADGE_SLOT => {
            let has_fx = !state.chain_order(req.channel_uuid).await.is_empty();
            Ok(sidebar_badge_tree(has_fx))
        }
        DETAIL_SLOT => Ok(detail_iframe_tree()),
        other => Err(RpcError {
            code: error_codes::INVALID_PARAMS,
            message: format!("channel_overlay.render: unknown surface_id {other}"),
            data: None,
        }),
    }
}

pub async fn handle_overlay_event(
    _state: &Arc<EffectsState>,
    host: Arc<HostClient>,
    params: Option<Value>,
) -> Result<Value, RpcError> {
    let evt: OverlayEventRequest = parse_params(params, "channel_overlay.event")?;
    if evt.surface_id == SIDEBAR_BADGE_SLOT && evt.event_id == OPEN_RACK_EVENT {
        let _ = host
            .ui_channel_overlay_focus(json!({
                "surface_id": DETAIL_SLOT,
                "channel_uuid": evt.channel_uuid,
            }))
            .await;
    }
    Ok(json!({}))
}

fn sidebar_badge_tree(has_fx: bool) -> Value {
    let tone = if has_fx { "primary" } else { "muted" };
    json!({
        "kind": "row",
        "gap": "sm",
        "align": "center",
        "children": [{
            "kind": "button",
            "id": OPEN_RACK_EVENT,
            "label": "",
            "icon": "fx",
            "variant": "ghost",
            "tone": tone,
        }],
    })
}

fn detail_iframe_tree() -> Value {
    json!({
        "kind": "iframe",
        "surface_id": DETAIL_SLOT,
        "height": 600,
    })
}

fn settings_tree() -> Value {
    json!({
        "kind": "section",
        "title": "Effects",
        "children": [{
            "kind": "text",
            "tone": "muted",
            "value": "Plugin discovery + install controls land alongside the iframe rack UI.",
        }],
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::effect::Effect;

    #[test]
    fn sidebar_badge_tone_reflects_chain_state() {
        let off = sidebar_badge_tree(false);
        assert_eq!(off["children"][0]["tone"], "muted");
        let on = sidebar_badge_tree(true);
        assert_eq!(on["children"][0]["tone"], "primary");
    }

    #[test]
    fn sidebar_badge_uses_fx_icon_and_open_rack_id() {
        let t = sidebar_badge_tree(true);
        assert_eq!(t["children"][0]["icon"], "fx");
        assert_eq!(t["children"][0]["id"], OPEN_RACK_EVENT);
        assert_eq!(t["children"][0]["kind"], "button");
    }

    #[test]
    fn detail_tree_is_iframe_node() {
        let t = detail_iframe_tree();
        assert_eq!(t["kind"], "iframe");
        assert_eq!(t["surface_id"], DETAIL_SLOT);
    }

    #[test]
    fn settings_tree_is_section() {
        let t = settings_tree();
        assert_eq!(t["kind"], "section");
    }

    #[tokio::test]
    async fn render_overlay_returns_muted_tone_for_empty_chain() {
        let state = EffectsState::new("test");
        let chan = Uuid::new_v4();
        let params = Some(json!({"surface_id": SIDEBAR_BADGE_SLOT, "channel_uuid": chan}));
        let v = render_overlay(&state, params).await.unwrap();
        assert_eq!(v["children"][0]["tone"], "muted");
    }

    #[tokio::test]
    async fn render_overlay_returns_primary_tone_when_chain_present() {
        let state = EffectsState::new("test");
        let chan = Uuid::new_v4();
        state.attach_effect(chan, Effect::new_lv2("uri"), 0).await;
        let params = Some(json!({"surface_id": SIDEBAR_BADGE_SLOT, "channel_uuid": chan}));
        let v = render_overlay(&state, params).await.unwrap();
        assert_eq!(v["children"][0]["tone"], "primary");
    }

    #[tokio::test]
    async fn render_overlay_for_detail_returns_iframe_node() {
        let state = EffectsState::new("test");
        let params = Some(json!({"surface_id": DETAIL_SLOT, "channel_uuid": Uuid::nil()}));
        let v = render_overlay(&state, params).await.unwrap();
        assert_eq!(v["kind"], "iframe");
    }

    #[tokio::test]
    async fn render_overlay_unknown_slot_is_invalid_params() {
        let state = EffectsState::new("test");
        let params = Some(json!({"surface_id": "nope", "channel_uuid": Uuid::nil()}));
        let err = render_overlay(&state, params).await.unwrap_err();
        assert_eq!(err.code, error_codes::INVALID_PARAMS);
    }
}
