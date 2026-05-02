//! XDG GlobalShortcuts portal listener (raw zbus).
//!
//! Wayland-native PTT capture path. The compositor manages the actual key
//! bindings; we register shortcut IDs and react to Activated/Deactivated
//! signals. No `/dev/input` permissions required.
//!
//! ## Why raw zbus instead of ashpd
//!
//! ashpd 0.13's `CreateSessionOptions` keeps `session_handle_token` private
//! and randomizes it via `Default`. KDE's xdg-desktop-portal-kde uses that
//! token verbatim as the section name in `~/.config/kglobalshortcutsrc`
//! (e.g. `[token_ashpd_az2xDtFqtU]`), so a fresh random token on every
//! launch creates a new entry — and any keys the user previously assigned
//! point at the old, abandoned entry.
//!
//! Using a stable token (`tideline_ptt`) means KDE consistently matches our
//! BindShortcuts call against the same persisted entry, and the user's
//! configured keys actually trigger our handlers.
//!
//! ## Flow
//!
//! ```text
//!   subscribe to Request.Response on predictable path
//!   call GlobalShortcuts.CreateSession with stable session token
//!   await Response → confirms session exists
//!   call GlobalShortcuts.BindShortcuts (also via stable request token)
//!   await Response → confirms shortcuts registered
//!   listen for Activated / Deactivated signals → dispatch
//! ```

use crate::runtime::PttRuntime;
use futures_util::StreamExt;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use zbus::{
    zvariant::{ObjectPath, OwnedObjectPath, OwnedValue, Value},
    Connection, Proxy,
};

pub const SHORTCUT_TOGGLE: &str = "mode_toggle";
pub const SHORTCUT_HOLD: &str = "hold";

const PORTAL_BUS: &str = "org.freedesktop.portal.Desktop";
const PORTAL_PATH: &str = "/org/freedesktop/portal/desktop";
const IFACE_GLOBAL_SHORTCUTS: &str = "org.freedesktop.portal.GlobalShortcuts";
const IFACE_REQUEST: &str = "org.freedesktop.portal.Request";

/// Stable handle tokens. Unlike ashpd's auto-randomized defaults, these
/// persist across launches so KDE re-uses the same shortcut entry.
pub const SESSION_TOKEN: &str = "tideline_ptt";
pub const REQ_TOKEN_CREATE: &str = "tideline_ptt_create";
pub const REQ_TOKEN_BIND: &str = "tideline_ptt_bind";

/// Attempt to register PTT shortcuts via the XDG GlobalShortcuts portal and
/// start a background listener. Returns Ok once shortcuts are bound.
///
/// On Err the caller should fall back to evdev. Common failure modes:
///   - portal not running / GlobalShortcuts unavailable
///   - bind rejected by the compositor
///   - timeout waiting for Response signal
pub async fn try_start(runtime: Arc<PttRuntime>) -> Result<(), String> {
    let conn = Connection::session()
        .await
        .map_err(|e| format!("dbus session connect: {}", e))?;

    let portal = Proxy::new(&conn, PORTAL_BUS, PORTAL_PATH, IFACE_GLOBAL_SHORTCUTS)
        .await
        .map_err(|e| format!("portal proxy: {}", e))?;

    let unique_id = unique_id(&conn)?;

    let mut create_opts: HashMap<&str, Value<'_>> = HashMap::new();
    create_opts.insert("handle_token", REQ_TOKEN_CREATE.into());
    create_opts.insert("session_handle_token", SESSION_TOKEN.into());
    let _create_resp = portal_request(
        &conn,
        &portal,
        "CreateSession",
        &(create_opts,),
        &unique_id,
        REQ_TOKEN_CREATE,
    )
    .await
    .map_err(|e| format!("CreateSession: {}", e))?;

    let session_path: OwnedObjectPath = ObjectPath::try_from(format!(
        "/org/freedesktop/portal/desktop/session/{}/{}",
        unique_id, SESSION_TOKEN
    ))
    .map_err(|e| format!("bad session path: {}", e))?
    .into();

    let shortcuts: Vec<(&str, HashMap<&str, Value<'_>>)> = vec![
        (
            SHORTCUT_TOGGLE,
            HashMap::from([(
                "description",
                Value::from("Toggle PTT mode (open mic / push-to-talk)"),
            )]),
        ),
        (
            SHORTCUT_HOLD,
            HashMap::from([("description", Value::from("Hold to transmit"))]),
        ),
    ];
    let parent_window = "";
    let mut bind_opts: HashMap<&str, Value<'_>> = HashMap::new();
    bind_opts.insert("handle_token", REQ_TOKEN_BIND.into());

    let _bind_resp = portal_request(
        &conn,
        &portal,
        "BindShortcuts",
        &(&session_path, shortcuts, parent_window, bind_opts),
        &unique_id,
        REQ_TOKEN_BIND,
    )
    .await
    .map_err(|e| format!("BindShortcuts: {}", e))?;

    tokio::spawn(async move {
        let _conn = conn.clone();
        let mut activated = match portal.receive_signal("Activated").await {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!(?e, "portal Activated stream");
                return;
            }
        };
        let mut deactivated = match portal.receive_signal("Deactivated").await {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!(?e, "portal Deactivated stream");
                return;
            }
        };
        loop {
            tokio::select! {
                msg = activated.next() => {
                    let Some(msg) = msg else { break };
                    if let Some(id) = parse_shortcut_id(&msg) {
                        match id.as_str() {
                            SHORTCUT_TOGGLE => runtime.toggle_mode().await,
                            SHORTCUT_HOLD => runtime.hold_press().await,
                            _ => {}
                        }
                    }
                }
                msg = deactivated.next() => {
                    let Some(msg) = msg else { break };
                    if let Some(id) = parse_shortcut_id(&msg) {
                        if id == SHORTCUT_HOLD { runtime.hold_release().await; }
                    }
                }
            }
        }
    });

    Ok(())
}

/// The portal's request/response pattern: caller subscribes to the
/// Request.Response signal on a predictable path, then invokes the method,
/// then awaits the signal. We must subscribe BEFORE the call to avoid
/// missing fast responses.
async fn portal_request<B>(
    conn: &Connection,
    portal: &Proxy<'_>,
    method: &str,
    body: &B,
    unique_id: &str,
    handle_token: &str,
) -> Result<HashMap<String, OwnedValue>, String>
where
    B: serde::Serialize + zbus::zvariant::DynamicType,
{
    let req_path = format!(
        "/org/freedesktop/portal/desktop/request/{}/{}",
        unique_id, handle_token
    );
    let req_proxy = Proxy::new(conn, PORTAL_BUS, req_path.as_str(), IFACE_REQUEST)
        .await
        .map_err(|e| format!("request proxy: {}", e))?;
    let mut response_stream = req_proxy
        .receive_signal("Response")
        .await
        .map_err(|e| format!("subscribe Response: {}", e))?;

    // Fire the method. Reply is the request object path; we don't need it
    // since we predicted it above.
    let _: OwnedObjectPath = portal
        .call(method, body)
        .await
        .map_err(|e| format!("portal call {}: {}", method, e))?;

    // Await the Response signal. 10s should be more than enough; if the
    // compositor is hung longer than that, fall back.
    let msg = tokio::time::timeout(Duration::from_secs(10), response_stream.next())
        .await
        .map_err(|_| format!("{} response timed out", method))?
        .ok_or_else(|| format!("{} response stream closed", method))?;

    let body = msg.body();
    let (code, results): (u32, HashMap<String, OwnedValue>) = body
        .deserialize()
        .map_err(|e| format!("response decode: {}", e))?;
    if code != 0 {
        return Err(format!("{} returned response code {}", method, code));
    }
    Ok(results)
}

fn unique_id(conn: &Connection) -> Result<String, String> {
    let unique_name = conn
        .unique_name()
        .ok_or("connection has no unique name")?
        .as_str()
        .to_string();
    Ok(unique_name.trim_start_matches(':').replace('.', "_"))
}

/// Both Activated and Deactivated signals carry `(o, s, t, a{sv})` =
/// (session_path, shortcut_id, timestamp, options). We only need the id.
fn parse_shortcut_id(msg: &zbus::Message) -> Option<String> {
    let body = msg.body();
    let parsed: Result<(OwnedObjectPath, String, u64, HashMap<String, OwnedValue>), _> =
        body.deserialize();
    parsed.ok().map(|(_, id, _, _)| id)
}
