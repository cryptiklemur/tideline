//! LV2 UI hosting via suil + xcb.
//!
//! Strategy: walk the plugin's UIs via lilv, pick an X11UI (the only widely
//! supported toolkit on Linux), then ask suil to instantiate it with a
//! `ui:parent` feature pointing at our top-level xcb window. Suil takes care
//! of dlopen'ing the UI binary and reparenting its X window into ours.
//!
//! Param writes flowing back from the UI go through the `SuilHost` write
//! callback into a [`UiController`] supplied by the engine. Plugin → UI
//! feedback (control output meters, atom output for graphs/spectrum) is
//! forwarded from the audio thread via an mpsc::sync_channel and pushed
//! into the UI in `idle()` via `suil_instance_port_event`.

use std::ffi::{c_char, c_void, CStr, CString};
use std::os::raw::c_int;
use std::ptr;
use std::sync::mpsc::{sync_channel, Receiver, SyncSender};
use std::sync::{Arc, Once};

use anyhow::Context;

use crate::host::lv2::{UiPluginEvent, UiToPluginAtom, UI_ATOM_QUEUE_DEPTH};
use crate::host::{ParentWindow, PluginUi, UiController};
use crate::suil_sys;
use crate::ui_bridge::PluginWindow;

const LV2_UI_X11_UI: &str = "http://lv2plug.in/ns/extensions/ui#X11UI";
const LV2_UI_PARENT: &str = "http://lv2plug.in/ns/extensions/ui#parent";
const LV2_UI_IDLE_INTERFACE: &str = "http://lv2plug.in/ns/extensions/ui#idleInterface";
/// Stub URI for the worker feature passed to `Features::iter_features` —
/// livi requires a worker feature to satisfy the iterator signature, but we
/// skip it when copying features over to suil since UIs don't need workers.
const LV2_WORKER_SCHEDULE_URI: &str = "http://lv2plug.in/ns/ext/worker#schedule";

/// LV2_Feature ABI mirror (avoid pulling in lv2_raw just for one type in this
/// module's local declarations — but lv2_raw::LV2Feature has the same repr(C)
/// layout, so we can cast pointers between them when copying out features
/// from livi's `iter_features`).
#[repr(C)]
struct LV2Feature {
    uri: *const c_char,
    data: *mut c_void,
}

/// LV2UI_Idle_Interface ABI: a vtable with a single `idle(handle)` entry.
#[repr(C)]
struct Lv2UiIdleInterface {
    idle: Option<unsafe extern "C" fn(ui: *mut c_void) -> c_int>,
}

static SUIL_INIT: Once = Once::new();

fn suil_init_once() {
    SUIL_INIT.call_once(|| {
        let mut argc: c_int = 0;
        let mut argv_storage: Vec<*mut c_char> = vec![ptr::null_mut()];
        let mut argv: *mut *mut c_char = argv_storage.as_mut_ptr();
        // SAFETY: suil_init mutates argc/argv pointers in-place but we hand it
        // empty stubs; it does not retain them past the call.
        unsafe {
            suil_sys::suil_init(
                &mut argc,
                &mut argv,
                suil_sys::SuilArg_SUIL_ARG_NONE,
            );
        }
    });
}

pub(super) fn open(
    plugin: &mut super::lv2::Lv2Plugin,
    parent: ParentWindow,
    controller: Arc<dyn UiController>,
) -> anyhow::Result<Box<dyn PluginUi>> {
    suil_init_once();

    let lilv_plugin = plugin.plugin_handle.raw().clone();
    let world_node_x11ui = plugin
        .world
        .raw()
        .new_uri(LV2_UI_X11_UI);

    let uis = lilv_plugin.uis().context("plugin has no UIs")?;
    let chosen = uis
        .iter()
        .find(|ui| ui.is_a(&world_node_x11ui))
        .context("no X11UI found for plugin (other UI toolkits not yet supported)")?;

    let ui_uri_node = chosen.uri();
    let ui_uri = ui_uri_node
        .as_uri()
        .context("UI uri node not a URI")?
        .to_string();
    let ui_type_uri = LV2_UI_X11_UI.to_string();
    let bundle_uri_node = chosen.bundle_uri().context("UI bundle uri missing")?;
    let bundle_uri = bundle_uri_node
        .as_uri()
        .context("UI bundle uri not a URI")?
        .to_string();
    let binary_uri_node = chosen.binary_uri().context("UI binary uri missing")?;
    let binary_uri = binary_uri_node
        .as_uri()
        .context("UI binary uri not a URI")?
        .to_string();

    // Suil expects filesystem paths, not file:// URIs. lilv hands us file://
    // for local plugins; strip the prefix when present.
    let bundle_path = file_uri_to_path(&bundle_uri);
    let binary_path = file_uri_to_path(&binary_uri);

    let plugin_uri = plugin.info.uri.clone();
    let event_transfer_urid = plugin.event_transfer_urid;

    drop(uis);
    drop(world_node_x11ui);

    // The controller is shared between this struct and the SuilHost C
    // callbacks. We leak it into a raw pointer for the C side and keep the
    // Arc alive in the returned Lv2PluginUi so the strong count stays ≥1
    // until the UI is closed.
    // Audio → UI feedback channel (control outs + atom outs).
    let (event_tx, event_rx) = sync_channel::<UiPluginEvent>(UI_ATOM_QUEUE_DEPTH);
    plugin.ui_event_tx = Some(event_tx);

    // UI → plugin atom channel. The audio thread drains this at the top of
    // each block and pushes the atoms into the plugin's atom_in port.
    let (atom_tx, atom_rx) = sync_channel::<UiToPluginAtom>(UI_ATOM_QUEUE_DEPTH);
    plugin.ui_atom_rx = Some(atom_rx);

    let controller_box = Box::new(ControllerHandle {
        controller: controller.clone(),
        ui_atom_tx: Some(atom_tx),
        event_transfer_urid,
    });
    let controller_ptr = Box::into_raw(controller_box);

    let host = unsafe {
        suil_sys::suil_host_new(
            Some(write_func),
            Some(index_func),
            None,
            None,
        )
    };
    if host.is_null() {
        unsafe {
            drop(Box::from_raw(controller_ptr));
        }
        anyhow::bail!("suil_host_new returned null");
    }

    // Build the LV2_Feature array suil_instance_new will hand to the UI.
    // We forward every feature livi exposes (urid#map, urid#unmap, options,
    // bounded-block-length) so the UI sees the same URID space as the audio
    // thread. LSP UIs in particular refuse to render meshes/spectrum without
    // urid#map + options + log-style features. The stub worker we hand to
    // iter_features is required to satisfy livi's iterator signature; we
    // skip it in the forwarded list since UIs don't need worker access.
    let stub_worker_uri = CString::new(LV2_WORKER_SCHEDULE_URI).unwrap();
    let stub_worker = lv2_raw::LV2Feature {
        uri: stub_worker_uri.as_ptr(),
        data: ptr::null_mut(),
    };

    let parent_window_id: usize = parent.x11_window as usize;
    let parent_uri = CString::new(LV2_UI_PARENT).unwrap();
    let parent_feature = LV2Feature {
        uri: parent_uri.as_ptr(),
        data: parent_window_id as *mut c_void,
    };

    let mut features: Vec<*const LV2Feature> = Vec::new();
    features.push(&parent_feature);
    for f in plugin.features.iter_features(&stub_worker) {
        let uri = unsafe { CStr::from_ptr(f.uri) }.to_string_lossy();
        if uri == LV2_WORKER_SCHEDULE_URI {
            continue;
        }
        let raw = f as *const lv2_raw::LV2Feature as *const LV2Feature;
        features.push(raw);
    }
    features.push(ptr::null());

    let container_type_c = CString::new(LV2_UI_X11_UI.as_bytes()).unwrap();
    let plugin_uri_c = CString::new(plugin_uri.as_bytes()).unwrap();
    let ui_uri_c = CString::new(ui_uri.as_bytes()).unwrap();
    let ui_type_c = CString::new(ui_type_uri.as_bytes()).unwrap();
    let bundle_path_c = CString::new(bundle_path.as_bytes()).unwrap();
    let binary_path_c = CString::new(binary_path.as_bytes()).unwrap();

    let instance = unsafe {
        suil_sys::suil_instance_new(
            host,
            controller_ptr as *mut c_void,
            container_type_c.as_ptr(),
            plugin_uri_c.as_ptr(),
            ui_uri_c.as_ptr(),
            ui_type_c.as_ptr(),
            bundle_path_c.as_ptr(),
            binary_path_c.as_ptr(),
            features.as_ptr() as *const *const suil_sys::LV2_Feature,
        )
    };
    if instance.is_null() {
        unsafe {
            suil_sys::suil_host_free(host);
            drop(Box::from_raw(controller_ptr));
        }
        anyhow::bail!("suil_instance_new returned null (UI failed to load)");
    }

    let idle_iface_c = CString::new(LV2_UI_IDLE_INTERFACE).unwrap();
    let idle_iface_raw = unsafe {
        suil_sys::suil_instance_extension_data(instance, idle_iface_c.as_ptr())
    };
    let idle_iface = if idle_iface_raw.is_null() {
        None
    } else {
        Some(idle_iface_raw as *const Lv2UiIdleInterface)
    };

    // For X11UI suil hands back the embedded widget's X window id cast to
    // *mut c_void. Capture it so the engine can query its natural size and
    // resize our parent window to fit.
    let widget_ptr = unsafe { suil_sys::suil_instance_get_widget(instance) };
    let widget_window_id = if widget_ptr.is_null() {
        None
    } else {
        Some(widget_ptr as usize as u32)
    };

    Ok(Box::new(Lv2PluginUi {
        host,
        instance,
        controller_ptr,
        controller_keepalive: controller,
        idle_iface,
        widget_window_id,
        event_rx,
        event_transfer_urid,
        _window: None,
    }))
}

fn file_uri_to_path(uri: &str) -> String {
    if let Some(rest) = uri.strip_prefix("file://") {
        rest.to_string()
    } else {
        uri.to_string()
    }
}

struct ControllerHandle {
    controller: Arc<dyn UiController>,
    /// UI → plugin atom forwarding. The audio thread drains this on each
    /// block and pushes the atoms into the plugin's atom_in[N] sequence.
    /// Null means the plugin has no atom inputs (so nothing to forward).
    ui_atom_tx: Option<SyncSender<UiToPluginAtom>>,
    /// URID for atom#eventTransfer — the protocol value suil uses when
    /// forwarding atom port writes from the UI.
    event_transfer_urid: u32,
}

unsafe extern "C" fn write_func(
    controller: *mut c_void,
    port_index: u32,
    buffer_size: u32,
    protocol: u32,
    buffer: *const c_void,
) {
    if controller.is_null() || buffer.is_null() {
        return;
    }
    let handle = &*(controller as *const ControllerHandle);
    if protocol == 0 && buffer_size == 4 {
        // Float control write from the UI (the user moved a knob).
        let value = *(buffer as *const f32);
        handle.controller.write_param(port_index, value);
        return;
    }
    if protocol == handle.event_transfer_urid {
        // Atom/event write from the UI — typically an LSP "subscribe to mesh
        // X" Object atom. Forward to the audio thread so it can splice it
        // into the matching atom_in port before the next plugin run.
        let Some(tx) = handle.ui_atom_tx.as_ref() else {
            return;
        };
        let bytes = std::slice::from_raw_parts(buffer as *const u8, buffer_size as usize);
        let _ = tx.try_send(UiToPluginAtom {
            port_index,
            data: bytes.to_vec(),
        });
        return;
    }
    // Unknown protocol — silently drop. Logging here would spam the audio
    // thread on hot UI paths and we have no way to recover anyway.
}

unsafe extern "C" fn index_func(
    _controller: *mut c_void,
    _port_symbol: *const c_char,
) -> u32 {
    // Not implemented yet. Suil only calls this for symbol→index lookups
    // that the UI explicitly requests; most LV2 UIs use port indices.
    u32::MAX
}

pub(super) struct Lv2PluginUi {
    host: *mut suil_sys::SuilHost,
    instance: *mut suil_sys::SuilInstance,
    controller_ptr: *mut ControllerHandle,
    /// Keeps the Arc alive so the leaked C pointer stays valid.
    controller_keepalive: Arc<dyn UiController>,
    idle_iface: Option<*const Lv2UiIdleInterface>,
    widget_window_id: Option<u32>,
    /// Plugin → UI feedback events from the audio thread, drained in idle().
    event_rx: Receiver<UiPluginEvent>,
    /// URID for atom#eventTransfer — the `format` arg for atom port events.
    event_transfer_urid: u32,
    /// Optional owning window if the host wants us to also manage it. The
    /// PluginWindow owning the parent x11 id lives in the engine layer.
    _window: Option<PluginWindow>,
}

unsafe impl Send for Lv2PluginUi {}

impl PluginUi for Lv2PluginUi {
    fn idle(&mut self) {
        // Drain any plugin → UI feedback. Forwarding before the UI's idle
        // callback runs lets the toolkit pick up the new values on the same
        // pump tick, avoiding a one-frame lag.
        while let Ok(ev) = self.event_rx.try_recv() {
            unsafe {
                match ev {
                    UiPluginEvent::Control { port_index, value } => {
                        let v = value;
                        suil_sys::suil_instance_port_event(
                            self.instance,
                            port_index,
                            4,
                            0,
                            &v as *const f32 as *const c_void,
                        );
                    }
                    UiPluginEvent::Atom { port_index, data } => {
                        suil_sys::suil_instance_port_event(
                            self.instance,
                            port_index,
                            data.len() as u32,
                            self.event_transfer_urid,
                            data.as_ptr() as *const c_void,
                        );
                    }
                }
            }
        }

        if let Some(iface) = self.idle_iface {
            unsafe {
                if let Some(idle_fn) = (*iface).idle {
                    let handle = suil_sys::suil_instance_get_handle(self.instance);
                    let _ = idle_fn(handle);
                }
            }
        }
    }

    fn resize(&mut self, _width: u32, _height: u32) {
        // Suil doesn't expose a portable resize entry. Most LV2UI hosts
        // resize the parent X window and let the UI follow via configure
        // events; the engine resizes ui_bridge::PluginWindow directly when
        // the user resizes.
    }

    fn widget_window_id(&self) -> Option<u32> {
        self.widget_window_id
    }
}

impl Drop for Lv2PluginUi {
    fn drop(&mut self) {
        unsafe {
            if !self.instance.is_null() {
                suil_sys::suil_instance_free(self.instance);
                self.instance = ptr::null_mut();
            }
            if !self.host.is_null() {
                suil_sys::suil_host_free(self.host);
                self.host = ptr::null_mut();
            }
            if !self.controller_ptr.is_null() {
                drop(Box::from_raw(self.controller_ptr));
                self.controller_ptr = ptr::null_mut();
            }
        }
        // Touch the keepalive so the compiler doesn't elide the field.
        let _ = &self.controller_keepalive;
    }
}

// CStr is referenced only inside the function above; pull it into scope to
// satisfy clippy's used_underscore_binding detection across compilers.
#[allow(dead_code)]
fn _touch_cstr(s: *const c_char) -> &'static CStr {
    unsafe { CStr::from_ptr(s) }
}
