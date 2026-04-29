//! Suppress noisy upstream `g_warning` calls we cannot otherwise silence.
//!
//! The libayatana-appindicator C library (pulled in transitively by Tauri's
//! tray-icon support on Linux) prints a deprecation warning via `g_warning`
//! the first time it is loaded:
//!
//! > libayatana-appindicator is deprecated. Please use
//! > libayatana-appindicator-glib in newly written code.
//!
//! That migration is upstream's problem, not ours, and the warning has no
//! functional effect — but it shows up on every launch. We register a
//! silent log handler for that specific log domain so it never reaches
//! stderr.
//!
//! libglib-2.0 is already loaded into our process by the tray-icon path,
//! so the FFI symbol is resolvable at runtime; no extra link flags or
//! crate deps are needed.

use std::ffi::CString;
use std::os::raw::{c_char, c_int, c_void};

#[link(name = "glib-2.0")]
unsafe extern "C" {
    fn g_log_set_handler(
        log_domain: *const c_char,
        log_levels: c_int,
        log_func: extern "C" fn(*const c_char, c_int, *const c_char, *mut c_void),
        user_data: *mut c_void,
    ) -> u32;
}

extern "C" fn drop_message(
    _domain: *const c_char,
    _level: c_int,
    _message: *const c_char,
    _user_data: *mut c_void,
) {
}

/// Silence all log messages from a single GLib log domain.
fn silence_domain(domain: &str) {
    let Ok(c) = CString::new(domain) else { return };
    // -1 = G_LOG_LEVEL_MASK | G_LOG_FATAL_MASK (every level + fatal flag).
    unsafe { g_log_set_handler(c.as_ptr(), -1, drop_message, std::ptr::null_mut()); }
}

/// Install handlers for the upstream libraries that spam stderr at startup.
pub fn suppress_upstream_warnings() {
    silence_domain("libayatana-appindicator");
}
