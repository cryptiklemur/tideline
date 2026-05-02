//! Safe-ish wrapper around carla-sys for the effects plugin.
//!
//! Carla's standalone API is single-host-per-process. We expose a single
//! `Host` handle initialized once via `Host::init()` and then engine /
//! plugin operations as methods on `&Host`.

use carla_sys::*;
use std::ffi::{CStr, CString};
use std::os::raw::c_uint;
use std::path::Path;

// Bindgen exposes BinaryType / PluginType as scoped consts under the
// CarlaBackend namespace. Re-bind the two values we need.
const BINARY_NATIVE: CarlaBackend_BinaryType = CarlaBackend_BinaryType_BINARY_POSIX64;
const PLUGIN_LV2: CarlaBackend_PluginType = CarlaBackend_PluginType_PLUGIN_LV2;

#[derive(Debug)]
pub struct Host {
    handle: CarlaHostHandle,
}

impl Host {
    pub fn init() -> Result<Self, String> {
        // SAFETY: carla_standalone_host_init is safe to call once per process.
        let handle = unsafe { carla_standalone_host_init() };
        if handle.is_null() {
            return Err("carla_standalone_host_init returned null".into());
        }
        Ok(Self { handle })
    }

    pub fn drivers(&self) -> Vec<String> {
        let count = unsafe { carla_get_engine_driver_count() };
        (0..count)
            .filter_map(|i| unsafe {
                let p = carla_get_engine_driver_name(i);
                if p.is_null() {
                    None
                } else {
                    Some(CStr::from_ptr(p).to_string_lossy().into_owned())
                }
            })
            .collect()
    }

    pub fn engine_init(&self, driver: &str, client_name: &str) -> Result<(), String> {
        let d = CString::new(driver).map_err(|e| e.to_string())?;
        let c = CString::new(client_name).map_err(|e| e.to_string())?;
        // SAFETY: pointers live for duration of call, handle is valid.
        let ok = unsafe { carla_engine_init(self.handle, d.as_ptr(), c.as_ptr()) };
        if ok {
            Ok(())
        } else {
            Err(format!("engine_init({driver}) failed"))
        }
    }

    pub fn engine_close(&self) -> bool {
        unsafe { carla_engine_close(self.handle) }
    }

    /// Add an LV2 plugin by URI. Returns the new plugin id (the count - 1
    /// after add, which is the carla convention).
    pub fn add_lv2(&self, uri: &str, name: &str) -> Result<u32, String> {
        let n = CString::new(name).map_err(|e| e.to_string())?;
        let u = CString::new(uri).map_err(|e| e.to_string())?;
        // SAFETY: arguments live for duration of call.
        let ok = unsafe {
            carla_add_plugin(
                self.handle,
                BINARY_NATIVE,
                PLUGIN_LV2,
                std::ptr::null(),
                n.as_ptr(),
                u.as_ptr(),
                0,
                std::ptr::null(),
                0,
            )
        };
        if !ok {
            return Err(format!("add_lv2({uri}) failed"));
        }
        let count = unsafe { carla_get_current_plugin_count(self.handle) };
        if count == 0 {
            return Err("plugin count was 0 after add".into());
        }
        Ok(count - 1)
    }

    pub fn set_active(&self, plugin_id: u32, on: bool) {
        unsafe { carla_set_active(self.handle, plugin_id as c_uint, on) }
    }

    pub fn set_parameter(&self, plugin_id: u32, param_id: u32, value: f32) {
        unsafe { carla_set_parameter_value(self.handle, plugin_id as c_uint, param_id, value) }
    }

    pub fn save_state(&self, plugin_id: u32, path: &Path) -> Result<(), String> {
        let p = CString::new(path.to_str().ok_or("non-utf8 path")?).map_err(|e| e.to_string())?;
        let ok = unsafe { carla_save_plugin_state(self.handle, plugin_id as c_uint, p.as_ptr()) };
        if ok {
            Ok(())
        } else {
            Err("save_plugin_state failed".into())
        }
    }

    pub fn load_state(&self, plugin_id: u32, path: &Path) -> Result<(), String> {
        let p = CString::new(path.to_str().ok_or("non-utf8 path")?).map_err(|e| e.to_string())?;
        let ok = unsafe { carla_load_plugin_state(self.handle, plugin_id as c_uint, p.as_ptr()) };
        if ok {
            Ok(())
        } else {
            Err("load_plugin_state failed".into())
        }
    }

    pub fn switch_plugins(&self, a: u32, b: u32) -> bool {
        unsafe { carla_switch_plugins(self.handle, a as c_uint, b as c_uint) }
    }

    pub fn remove(&self, plugin_id: u32) -> bool {
        unsafe { carla_remove_plugin(self.handle, plugin_id as c_uint) }
    }

    pub fn show_custom_ui(&self, plugin_id: u32, show: bool) {
        unsafe { carla_show_custom_ui(self.handle, plugin_id as c_uint, show) }
    }
}
