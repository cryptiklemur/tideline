//! Native top-level X11 window for hosting plugin UIs.
//!
//! Plugin UIs (LV2 X11UI, VST3, etc.) want to be embedded into a parent
//! native widget. We create one top-level xcb window per plugin instance and
//! hand its window id off through `ParentWindow { x11_window }`. The window
//! lives until [`PluginWindow::close`] is dropped or [`close`] is called.
//!
//! We also intercept WM_DELETE_WINDOW so the user clicking the WM close
//! button gives us a ClientMessage instead of letting the WM destroy our
//! window out from under the still-running LV2 UI (which causes X protocol
//! errors and process death).

use std::sync::Arc;

use anyhow::Context;
use parking_lot::Mutex;
use xcb::Xid;

use crate::host::ParentWindow;

/// Owns a top-level xcb window. The window is destroyed when the struct is
/// dropped; until then `parent()` returns the embedding handle suitable for
/// LV2 X11UI / VST3 hosts.
pub struct PluginWindow {
    conn: Arc<xcb::Connection>,
    window: xcb::x::Window,
    wm_delete_window: xcb::x::Atom,
    closed: Mutex<bool>,
}

impl PluginWindow {
    pub fn open(title: &str, width: u16, height: u16) -> anyhow::Result<Self> {
        let (conn, screen_num) = xcb::Connection::connect(None).context("xcb connect")?;
        let setup = conn.get_setup();
        let screen = setup
            .roots()
            .nth(screen_num as usize)
            .context("xcb screen lookup")?;
        let window: xcb::x::Window = conn.generate_id();
        conn.send_and_check_request(&xcb::x::CreateWindow {
            depth: xcb::x::COPY_FROM_PARENT as u8,
            wid: window,
            parent: screen.root(),
            x: 0,
            y: 0,
            width,
            height,
            border_width: 0,
            class: xcb::x::WindowClass::InputOutput,
            visual: screen.root_visual(),
            value_list: &[
                xcb::x::Cw::BackPixel(screen.white_pixel()),
                xcb::x::Cw::EventMask(
                    xcb::x::EventMask::EXPOSURE | xcb::x::EventMask::STRUCTURE_NOTIFY,
                ),
            ],
        })
        .context("xcb create_window")?;
        conn.send_and_check_request(&xcb::x::ChangeProperty {
            mode: xcb::x::PropMode::Replace,
            window,
            property: xcb::x::ATOM_WM_NAME,
            r#type: xcb::x::ATOM_STRING,
            data: title.as_bytes(),
        })
        .context("xcb set wm_name")?;

        // Intern WM_PROTOCOLS / WM_DELETE_WINDOW so the WM sends us a
        // ClientMessage on close instead of nuking the window.
        let proto_cookie = conn.send_request(&xcb::x::InternAtom {
            only_if_exists: false,
            name: b"WM_PROTOCOLS",
        });
        let del_cookie = conn.send_request(&xcb::x::InternAtom {
            only_if_exists: false,
            name: b"WM_DELETE_WINDOW",
        });
        let wm_protocols = conn
            .wait_for_reply(proto_cookie)
            .context("intern WM_PROTOCOLS")?
            .atom();
        let wm_delete_window = conn
            .wait_for_reply(del_cookie)
            .context("intern WM_DELETE_WINDOW")?
            .atom();
        conn.send_and_check_request(&xcb::x::ChangeProperty {
            mode: xcb::x::PropMode::Replace,
            window,
            property: wm_protocols,
            r#type: xcb::x::ATOM_ATOM,
            data: &[wm_delete_window],
        })
        .context("set WM_PROTOCOLS")?;

        conn.send_and_check_request(&xcb::x::MapWindow { window })
            .context("xcb map_window")?;
        conn.flush().context("xcb flush")?;
        Ok(Self {
            conn: Arc::new(conn),
            window,
            wm_delete_window,
            closed: Mutex::new(false),
        })
    }

    pub fn parent(&self) -> ParentWindow {
        ParentWindow {
            x11_window: self.window.resource_id(),
        }
    }

    /// Query the size of an arbitrary X11 window (typically the LV2 UI's
    /// embedded widget). Returns None on failure (window vanished etc).
    pub fn query_child_size(&self, child_window_id: u32) -> Option<(u16, u16)> {
        let cookie = self.conn.send_request(&xcb::x::GetGeometry {
            drawable: xcb::x::Drawable::Window(<xcb::x::Window as xcb::XidNew>::new(
                child_window_id,
            )),
        });
        let reply = self.conn.wait_for_reply(cookie).ok()?;
        Some((reply.width(), reply.height()))
    }

    /// Resize the top-level window. LV2 UIs that emit `ui:resize` calls
    /// (which we expose as a feature) end up here.
    pub fn resize(&self, width: u16, height: u16) {
        if width == 0 || height == 0 {
            return;
        }
        let _ = self.conn.send_request_checked(&xcb::x::ConfigureWindow {
            window: self.window,
            value_list: &[
                xcb::x::ConfigWindow::Width(width as u32),
                xcb::x::ConfigWindow::Height(height as u32),
            ],
        });
        let _ = self.conn.flush();
    }

    /// Drain pending xcb events. Returns `true` if the WM asked us to close
    /// (i.e. user clicked the title-bar X). The caller is responsible for
    /// dropping this `PluginWindow` (and the LV2 UI hosted in it) afterwards.
    pub fn pump_events(&self) -> bool {
        let mut wants_close = false;
        loop {
            match self.conn.poll_for_event() {
                Ok(Some(xcb::Event::X(xcb::x::Event::ClientMessage(cm)))) => {
                    if let xcb::x::ClientMessageData::Data32(data) = cm.data() {
                        if data[0] == self.wm_delete_window.resource_id() {
                            wants_close = true;
                        }
                    }
                }
                Ok(Some(_)) => continue,
                Ok(None) => break,
                Err(_) => break,
            }
        }
        wants_close
    }

    pub fn close(&self) {
        let mut closed = self.closed.lock();
        if *closed {
            return;
        }
        let _ = self.conn.send_and_check_request(&xcb::x::DestroyWindow {
            window: self.window,
        });
        let _ = self.conn.flush();
        *closed = true;
    }
}

impl Drop for PluginWindow {
    fn drop(&mut self) {
        self.close();
    }
}
