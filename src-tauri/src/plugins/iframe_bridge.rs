use std::sync::Arc;

use tauri::{
    http::{Request, Response, StatusCode},
    AppHandle, Emitter, Manager, Runtime, UriSchemeContext,
};
use tideline_host::{IframeBridge, PluginRegistry};

const SHIM_JS: &str = include_str!("../../assets/tideline_plugin_shim.js");

/// Handle a `tideline-plugin://<plugin_id>/<surface_id>/<rel>` request:
/// look up the iframe surface contribution, read the asset, and inject the
/// host shim into HTML responses so plugin code can call `window.tideline.*`.
pub fn handle_request<R: Runtime>(
    ctx: UriSchemeContext<'_, R>,
    req: Request<Vec<u8>>,
) -> Response<Vec<u8>> {
    let app = ctx.app_handle();
    let registry: tauri::State<'_, Arc<PluginRegistry>> = app.state();
    let url = req.uri().clone();
    let host = url.host().unwrap_or_default().to_string();
    let path = url.path().to_string();

    if host.is_empty() {
        return error_response(StatusCode::BAD_REQUEST, "missing plugin id");
    }

    if path == "/__tideline/shim.js" {
        return Response::builder()
            .status(StatusCode::OK)
            .header("content-type", "application/javascript; charset=utf-8")
            .header("cache-control", "no-store")
            .body(SHIM_JS.as_bytes().to_vec())
            .unwrap();
    }

    let surfaces = tauri::async_runtime::block_on(registry.contributions()).iframe_surfaces;
    let surface = surfaces
        .iter()
        .find(|s| s.plugin_id == host && path.starts_with(&format!("/{}/", s.surface_id)))
        .cloned();

    let surface = match surface {
        Some(s) => s,
        None => return error_response(StatusCode::NOT_FOUND, "unknown surface"),
    };

    let rel = path
        .trim_start_matches('/')
        .strip_prefix(&surface.surface_id)
        .unwrap_or("")
        .trim_start_matches('/');
    let entry_dir = std::path::Path::new(&surface.entry_path)
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_default();
    let target = if rel.is_empty() {
        std::path::PathBuf::from(&surface.entry_path)
    } else {
        let candidate = entry_dir.join(rel);
        match candidate.canonicalize() {
            Ok(c) if c.starts_with(&entry_dir) => c,
            _ => return error_response(StatusCode::FORBIDDEN, "path escape"),
        }
    };

    let bytes = match std::fs::read(&target) {
        Ok(b) => b,
        Err(_) => return error_response(StatusCode::NOT_FOUND, "asset"),
    };
    let mime = mime_guess::from_path(&target)
        .first_or_octet_stream()
        .to_string();

    let body = if mime.starts_with("text/html") {
        let html = String::from_utf8_lossy(&bytes);
        let injected = format!(
            "<script>window.__TIDELINE_PLUGIN__={{plugin_id:{:?},surface_id:{:?}}};</script>\
             <script src=\"/__tideline/shim.js\"></script>\n{}",
            surface.plugin_id, surface.surface_id, html
        );
        injected.into_bytes()
    } else {
        bytes
    };

    Response::builder()
        .status(StatusCode::OK)
        .header("content-type", mime)
        .header("cache-control", "no-store")
        .body(body)
        .unwrap()
}

fn error_response(code: StatusCode, msg: &str) -> Response<Vec<u8>> {
    Response::builder()
        .status(code)
        .header("content-type", "text/plain")
        .body(msg.as_bytes().to_vec())
        .unwrap()
}

/// Tauri command invoked by the in-iframe shim to forward a postMessage from
/// the plugin webview to the plugin runtime.
#[tauri::command]
pub async fn tideline_plugin_iframe_send<R: Runtime>(
    app: AppHandle<R>,
    plugin_id: String,
    surface_id: String,
    message: serde_json::Value,
) -> Result<(), String> {
    let bridge = app.state::<IframeBridge>().inner().clone();
    bridge.send_message(&plugin_id, &surface_id, message).await
}

/// Forward an iframe message from the host to the matching webview by emitting
/// a per-(plugin, surface) Tauri event. The Svelte iframe wrapper subscribes to
/// this event and posts the payload into the iframe via `postMessage`.
pub fn forward_to_iframe<R: Runtime>(
    app: &AppHandle<R>,
    plugin_id: &str,
    surface_id: &str,
    message: serde_json::Value,
) -> Result<(), String> {
    let topic = format!("tideline-plugin:iframe:{}:{}", plugin_id, surface_id);
    app.emit(&topic, message).map_err(|e| e.to_string())
}
