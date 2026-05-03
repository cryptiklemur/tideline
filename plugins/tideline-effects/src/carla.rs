//! Safe-ish wrapper around carla-sys for the effects plugin.
//!
//! Carla's standalone API is single-host-per-process. We expose a single
//! `Host` handle initialized once via `Host::init()` and then engine /
//! plugin operations as methods on `&Host`.

use carla_sys::*;
use std::ffi::{CStr, CString};
use std::os::raw::c_uint;
use std::path::Path;
use thiserror::Error;

// Bindgen exposes BinaryType / PluginType as scoped consts under the
// CarlaBackend namespace. Re-bind the two values we need.
const BINARY_NATIVE: CarlaBackend_BinaryType = CarlaBackend_BinaryType_BINARY_POSIX64;
const PLUGIN_LV2: CarlaBackend_PluginType = CarlaBackend_PluginType_PLUGIN_LV2;

#[derive(Debug, Error)]
pub enum CarlaError {
    #[error("carla_standalone_host_init returned null")]
    HostInit,
    #[error("engine_init({driver}) failed")]
    EngineInit { driver: String },
    #[error("engine_close failed")]
    EngineClose,
    #[error("add_lv2({uri}) failed")]
    AddLv2 { uri: String },
    #[error("save_plugin_state failed")]
    SaveState,
    #[error("load_plugin_state failed")]
    LoadState,
    #[error("invalid utf8 path")]
    Utf8Path,
    #[error("ffi: {0}")]
    Ffi(String),
}

#[derive(Debug)]
pub struct Host {
    handle: CarlaHostHandle,
}

impl Host {
    pub fn init() -> Result<Self, CarlaError> {
        // SAFETY: carla_standalone_host_init is safe to call once per process.
        let handle = unsafe { carla_standalone_host_init() };
        if handle.is_null() {
            return Err(CarlaError::HostInit);
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

    pub fn engine_init(&self, driver: &str, client_name: &str) -> Result<(), CarlaError> {
        let d = CString::new(driver).map_err(|e| CarlaError::Ffi(e.to_string()))?;
        let c = CString::new(client_name).map_err(|e| CarlaError::Ffi(e.to_string()))?;
        // SAFETY: pointers live for duration of call, handle is valid.
        let ok = unsafe { carla_engine_init(self.handle, d.as_ptr(), c.as_ptr()) };
        if ok {
            Ok(())
        } else {
            Err(CarlaError::EngineInit {
                driver: driver.to_string(),
            })
        }
    }

    pub fn engine_close(&self) -> bool {
        unsafe { carla_engine_close(self.handle) }
    }

    /// Number of plugins currently loaded in the engine.
    pub fn current_plugin_count(&self) -> u32 {
        // SAFETY: handle is valid for the lifetime of Host.
        unsafe { carla_get_current_plugin_count(self.handle) }
    }

    /// Number of parameters exposed by a plugin id.
    pub fn parameter_count(&self, plugin_id: u32) -> u32 {
        // SAFETY: handle is valid for the lifetime of Host.
        unsafe { carla_get_parameter_count(self.handle, plugin_id as c_uint) }
    }

    /// Add an LV2 plugin by URI. Returns the new plugin id (the count - 1
    /// after add, which is the carla convention).
    pub fn add_lv2(&self, uri: &str, name: &str) -> Result<u32, CarlaError> {
        let n = CString::new(name).map_err(|e| CarlaError::Ffi(e.to_string()))?;
        let u = CString::new(uri).map_err(|e| CarlaError::Ffi(e.to_string()))?;
        let before = self.current_plugin_count();
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
            return Err(CarlaError::AddLv2 {
                uri: uri.to_string(),
            });
        }
        let after = self.current_plugin_count();
        if after != before + 1 {
            return Err(CarlaError::Ffi(format!(
                "count drifted: before={before} after={after}"
            )));
        }
        Ok(after - 1)
    }

    pub fn set_active(&self, plugin_id: u32, on: bool) {
        unsafe { carla_set_active(self.handle, plugin_id as c_uint, on) }
    }

    pub fn set_parameter(&self, plugin_id: u32, param_id: u32, value: f32) {
        unsafe { carla_set_parameter_value(self.handle, plugin_id as c_uint, param_id, value) }
    }

    pub fn save_state(&self, plugin_id: u32, path: &Path) -> Result<(), CarlaError> {
        let p = CString::new(path.to_str().ok_or(CarlaError::Utf8Path)?)
            .map_err(|e| CarlaError::Ffi(e.to_string()))?;
        let ok = unsafe { carla_save_plugin_state(self.handle, plugin_id as c_uint, p.as_ptr()) };
        if ok {
            Ok(())
        } else {
            Err(CarlaError::SaveState)
        }
    }

    pub fn load_state(&self, plugin_id: u32, path: &Path) -> Result<(), CarlaError> {
        let p = CString::new(path.to_str().ok_or(CarlaError::Utf8Path)?)
            .map_err(|e| CarlaError::Ffi(e.to_string()))?;
        let ok = unsafe { carla_load_plugin_state(self.handle, plugin_id as c_uint, p.as_ptr()) };
        if ok {
            Ok(())
        } else {
            Err(CarlaError::LoadState)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_init_and_drivers() {
        let host = Host::init().expect("host init");
        let drivers = host.drivers();
        assert!(!drivers.is_empty(), "expected at least one driver");
    }

    #[test]
    fn carla_error_displays() {
        let e = CarlaError::AddLv2 {
            uri: "http://x".into(),
        };
        assert_eq!(format!("{e}"), "add_lv2(http://x) failed");
    }
}
