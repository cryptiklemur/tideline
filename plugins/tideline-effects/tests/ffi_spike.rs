//! Carla 2.6+ FFI viability spike — verifies the API surface we depend on
//! works end-to-end against libcarla_standalone2.so.
//!
//! Run with `cargo test -p tideline-effects --test ffi_spike -- --ignored --nocapture`.
//! Requires libcarla_standalone2.so on the system (carla-git 2.6.0+).
//!
//! This test uses the "Dummy" driver if available so it doesn't require a
//! running JACK/PipeWire server. If only audio drivers are available, it
//! tries the first one and skips with a diagnostic on engine_init failure.

use std::path::PathBuf;
use tempfile::tempdir;
use tideline_effects::carla::Host;

#[test]
#[ignore]
fn carla_ffi_round_trip() {
    let host = Host::init().expect("host_init");
    let drivers = host.drivers();
    assert!(!drivers.is_empty(), "expected at least one carla driver");
    eprintln!("available drivers: {drivers:?}");

    // Prefer Dummy if present; otherwise first available.
    let driver = drivers
        .iter()
        .find(|d| d.eq_ignore_ascii_case("Dummy"))
        .cloned()
        .unwrap_or_else(|| drivers[0].clone());
    eprintln!("using driver: {driver}");

    if let Err(e) = host.engine_init(&driver, "tideline-ffi-spike") {
        eprintln!("engine_init failed: {e}; SKIP (no working audio backend)");
        return;
    }

    // 1. add: lsp-plugins gate (mono). Falls back to a couple of well-known LV2 URIs.
    let candidate_uris = [
        "http://lsp-plug.in/plugins/lv2/gate_mono",
        "http://calf.sourceforge.net/plugins/Filter",
    ];
    let mut added = None;
    for uri in candidate_uris {
        match host.add_lv2(uri, "spike") {
            Ok(id) => {
                added = Some(id);
                eprintln!("added {uri} as plugin id {id}");
                break;
            }
            Err(e) => eprintln!("add {uri} failed: {e}; trying next"),
        }
    }
    let plugin_id = match added {
        Some(id) => id,
        None => {
            eprintln!("no candidate LV2 plugin loaded; SKIP");
            let _ = host.engine_close();
            return;
        }
    };

    // 2. set_active false then true
    host.set_active(plugin_id, false);
    host.set_active(plugin_id, true);

    // 3. set_parameter — best-effort; if the param doesn't exist carla just no-ops
    host.set_parameter(plugin_id, 0, 0.5);

    // 4. save state
    let dir = tempdir().expect("tempdir");
    let state_path: PathBuf = dir.path().join("state.xml");
    host.save_state(plugin_id, &state_path).expect("save_state");
    assert!(state_path.exists(), "state file should exist after save");
    let saved = std::fs::read_to_string(&state_path).expect("read state");
    assert!(!saved.is_empty(), "saved state must be non-empty");
    eprintln!("saved state ({} bytes)", saved.len());

    // 5. load state — round-trip
    host.load_state(plugin_id, &state_path).expect("load_state");

    // 6. switch_plugins (no-op with a single plugin, but must accept)
    let _ = host.switch_plugins(plugin_id, plugin_id);

    // 7. remove
    assert!(host.remove(plugin_id), "remove plugin");

    assert!(host.engine_close(), "engine_close");
}
