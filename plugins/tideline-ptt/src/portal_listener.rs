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

use futures_util::StreamExt;
use std::collections::HashMap;
use std::time::Duration;
use zbus::{
    zvariant::{OwnedObjectPath, OwnedValue},
    Connection, Proxy,
};

pub const SHORTCUT_TOGGLE: &str = "mode_toggle";
pub const SHORTCUT_HOLD: &str = "hold";

const PORTAL_BUS: &str = "org.freedesktop.portal.Desktop";
#[allow(dead_code)]
const PORTAL_PATH: &str = "/org/freedesktop/portal/desktop";
#[allow(dead_code)]
const IFACE_GLOBAL_SHORTCUTS: &str = "org.freedesktop.portal.GlobalShortcuts";
const IFACE_REQUEST: &str = "org.freedesktop.portal.Request";

/// Stable handle tokens. Unlike ashpd's auto-randomized defaults, these
/// persist across launches so KDE re-uses the same shortcut entry.
pub const SESSION_TOKEN: &str = "tideline_ptt";
#[allow(dead_code)]
pub const REQ_TOKEN_CREATE: &str = "tideline_ptt_create";
#[allow(dead_code)]
pub const REQ_TOKEN_BIND: &str = "tideline_ptt_bind";

// TODO(W6.T8): wire to runtime — re-enable `try_start`, `dispatch_activated`
// and `dispatch_deactivated` once `crate::runtime::PttRuntime` exists.
//
// The native version lives at `src-tauri/src/ptt/portal_listener.rs`. In the
// plugin port:
//   - drop the `app: AppHandle` parameter (no Tauri in the plugin process)
//   - replace `handle_toggle(&app, &runtime)` with `runtime.toggle_mode()`
//   - replace `handle_press(&app, &runtime)` with `runtime.hold_press()`
//   - replace `handle_release(&app, &runtime)` with `runtime.hold_release()`
//   - use plain `tokio::spawn` instead of `tauri::async_runtime::spawn`
//
// Skeleton (kept here for ease of restoration in T8):
//
//   pub async fn try_start(runtime: Arc<PttRuntime>) -> Result<(), String> {
//       let conn = Connection::session().await
//           .map_err(|e| format!("dbus session connect: {}", e))?;
//       let portal = Proxy::new(&conn, PORTAL_BUS, PORTAL_PATH, IFACE_GLOBAL_SHORTCUTS)
//           .await
//           .map_err(|e| format!("portal proxy: {}", e))?;
//       let unique_id = unique_id(&conn)?;
//
//       let mut create_opts: HashMap<&str, Value<'_>> = HashMap::new();
//       create_opts.insert("handle_token", REQ_TOKEN_CREATE.into());
//       create_opts.insert("session_handle_token", SESSION_TOKEN.into());
//       let _create_resp = portal_request(&conn, &portal, "CreateSession",
//           &(create_opts,), &unique_id, REQ_TOKEN_CREATE)
//           .await
//           .map_err(|e| format!("CreateSession: {}", e))?;
//
//       let session_path: OwnedObjectPath = ObjectPath::try_from(format!(
//           "/org/freedesktop/portal/desktop/session/{}/{}",
//           unique_id, SESSION_TOKEN
//       ))
//       .map_err(|e| format!("bad session path: {}", e))?
//       .into();
//
//       let shortcuts: Vec<(&str, HashMap<&str, Value<'_>>)> = vec![
//           (SHORTCUT_TOGGLE, HashMap::from([
//               ("description", Value::from("Toggle PTT mode (open mic / push-to-talk)")),
//           ])),
//           (SHORTCUT_HOLD, HashMap::from([
//               ("description", Value::from("Hold to transmit")),
//           ])),
//       ];
//       let parent_window = "";
//       let mut bind_opts: HashMap<&str, Value<'_>> = HashMap::new();
//       bind_opts.insert("handle_token", REQ_TOKEN_BIND.into());
//       let _bind_resp = portal_request(&conn, &portal, "BindShortcuts",
//           &(&session_path, shortcuts, parent_window, bind_opts),
//           &unique_id, REQ_TOKEN_BIND)
//           .await
//           .map_err(|e| format!("BindShortcuts: {}", e))?;
//
//       tokio::spawn(async move {
//           let _conn = conn.clone();
//           let mut activated = match portal.receive_signal("Activated").await {
//               Ok(s) => s,
//               Err(e) => { eprintln!("portal Activated stream: {}", e); return; }
//           };
//           let mut deactivated = match portal.receive_signal("Deactivated").await {
//               Ok(s) => s,
//               Err(e) => { eprintln!("portal Deactivated stream: {}", e); return; }
//           };
//           loop {
//               tokio::select! {
//                   msg = activated.next() => {
//                       let Some(msg) = msg else { break };
//                       if let Some(id) = parse_shortcut_id(&msg) {
//                           dispatch_activated(&id, &runtime);
//                       }
//                   }
//                   msg = deactivated.next() => {
//                       let Some(msg) = msg else { break };
//                       if let Some(id) = parse_shortcut_id(&msg) {
//                           dispatch_deactivated(&id, &runtime);
//                       }
//                   }
//               }
//           }
//       });
//       Ok(())
//   }
//
//   fn dispatch_activated(id: &str, runtime: &Arc<PttRuntime>) {
//       match id {
//           SHORTCUT_TOGGLE => runtime.toggle_mode(),
//           SHORTCUT_HOLD => runtime.hold_press(),
//           _ => {}
//       }
//   }
//
//   fn dispatch_deactivated(id: &str, runtime: &Arc<PttRuntime>) {
//       if id == SHORTCUT_HOLD { runtime.hold_release(); }
//   }

/// The portal's request/response pattern: caller subscribes to the
/// Request.Response signal on a predictable path, then invokes the method,
/// then awaits the signal. We must subscribe BEFORE the call to avoid
/// missing fast responses.
#[allow(dead_code)]
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

#[allow(dead_code)]
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
#[allow(dead_code)]
fn parse_shortcut_id(msg: &zbus::Message) -> Option<String> {
    let body = msg.body();
    let parsed: Result<(OwnedObjectPath, String, u64, HashMap<String, OwnedValue>), _> =
        body.deserialize();
    parsed.ok().map(|(_, id, _, _)| id)
}
