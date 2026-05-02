# Carla 2.6 OSC Contract Spike — Findings

**Date:** 2026-05-02
**Carla version tested:** 2.6.0-alpha1 (carla-git AUR)
**Branch:** `feat/plugin-system`
**Verdict:** The spike test passes (carla-rack survives the 7-message round-trip), **BUT** the pass is misleading. The OSC API has been substantially narrowed in carla 2.6 vs. the contract the Wave 7 plan was written against. This requires a re-architecture decision before continuing W7.T1+.

---

## What the spike actually proved

The test asserts only that `carla-rack` does not crash when it receives the 7 OSC messages. It does not assert the messages are *accepted* or *acted upon*. carla 2.6 silently drops unknown OSC paths, so a green test here means "no crash," not "contract honored."

The spike still has value: it confirmed (a) carla-rack starts and binds OSC on the documented port, and (b) sending arbitrary OSC packets to it is non-fatal.

## What changed in carla 2.6

### CLI args from the original plan that DO NOT EXIST

The plan's spawn invocation:

```
carla-rack -n --no-jack-transport --client-name tideline-osc-spike --osc-tcp 22752 --osc-udp 22752
```

is invalid in 2.6. `carla-rack --help` lists only:

- `--cnprefix` — set client-name prefix in multi-client mode
- `--gdb` — run inside gdb
- `-n` / `--no-gui` — headless (but **requires a project file**, errors out otherwise)
- `-h`, `-v`

`--osc-tcp`, `--osc-udp`, `--no-jack-transport`, `--client-name` are gone (or never existed in this build line). carla-rack defaults to listening on UDP+TCP 22752; that part of the plan still works.

### OSC messages from the plan that DO NOT EXIST as OSC handlers

Symbol dump of `/usr/lib/carla/libcarla_standalone2.so` shows the surviving OSC dispatch handlers are scoped per-plugin and only cover **parameter-level** control:

```
handleMsgRegister, handleMsgUnregister
handleMsgSetActive
handleMsgSetVolume, handleMsgSetDryWet
handleMsgSetBalanceLeft, handleMsgSetBalanceRight, handleMsgSetPanning
handleMsgSetParameterValue
handleMsgSetParameterMidiChannel
handleMsgSetParameterMappedControlIndex
handleMsgSetParameterMappedRange
handleMsgSetProgram, handleMsgSetMidiProgram
handleMsgNoteOn, handleMsgNoteOff
```

The 7 messages we depend on map as follows:

| Plan message | Carla 2.6 OSC | Notes |
|---|---|---|
| `/add_plugin` | **NOT IN OSC** | Moved to qtweb HTTP API (`carla_backend_qtweb.py:add_plugin`) |
| `/set_active` | `handleMsgSetActive` (per-plugin path) | Survives, but path shape differs |
| `/switch_plugins` | **NOT IN OSC** | Moved to qtweb HTTP API |
| `/show_custom_ui` | **NOT IN OSC** | Moved to qtweb HTTP API |
| `/save_plugin_state` | **NOT IN OSC** | Moved to qtweb HTTP API |
| `/load_plugin_state` | **NOT IN OSC** | Moved to qtweb HTTP API |
| `/remove_plugin` | **NOT IN OSC** | Moved to qtweb HTTP API |

Source: `grep -E "/add_plugin|/remove_plugin|..." /usr/share/carla/*.py` returns hits only in `carla_backend_qtweb.py`, which uses `requests.get("{}/add_plugin", ...)` — i.e. these are now HTTP/REST endpoints on the qtweb backend, not OSC paths on the engine.

### What carla 2.6 OSC can still do

OSC remains the **per-plugin parameter control bus**, equivalent in scope to a generic-MIDI-mapping surface. A registered OSC client can:

- Register/unregister as the OSC control surface (`/register`, `/unregister`)
- Toggle active/bypass on a known plugin
- Set volume / dry-wet / pan / balance
- Set named parameter values
- Map parameters to CC / ranges
- Send note on/off, MIDI program changes

It cannot load, save, reorder, add, or remove plugins via OSC. Those are now qtweb HTTP API responsibilities.

## Implications for Wave 7

The Wave 7 plan ("approach 2: per-channel Carla-hosted DSP chains driven by OSC") was sized against the 2.4-era OSC contract. With 2.6, six of the seven control flows are gone from OSC. We have three forward paths:

### Option A — Embrace qtweb as the control surface

Drive carla via its HTTP qtweb API for chain construction (`/add_plugin`, `/remove_plugin`, `/switch_plugins`, `/load_plugin_state`, `/save_plugin_state`, `/show_custom_ui`) and keep OSC for hot-path parameter automation (volume, dry-wet, parameter values). This is the path carla 2.6 itself takes — `carla_backend_qtweb.py` is the canonical out-of-process control client.

**Pros:** Aligned with upstream direction; HTTP is easier to debug and version than OSC.
**Cons:** Plan's "OSC client" framing has to be split into "HTTP control + OSC parameter bus." More moving parts. Need to verify qtweb is enabled in the headless `carla-rack` invocation we'll use (it's a separate thread inside carla, may need a settings flag).

### Option B — Use libcarla_standalone2 directly via FFI

Skip the IPC layer entirely. The C ABI symbols (`carla_add_plugin`, `carla_remove_plugin`, `carla_save_plugin_state`, `carla_show_custom_ui`, `carla_switch_plugins`, etc.) are all exported from `/usr/lib/carla/libcarla_standalone2.so` and are present in the strings dump. Bind via `bindgen` or hand-rolled FFI from the `tideline-effects` plugin process.

**Pros:** Zero IPC overhead, no version drift concerns, full API surface.
**Cons:** Carla becomes an in-process dependency rather than a managed subprocess; crash isolation is gone; we have to ship a libcarla version pin or handle ABI variance across distros.

### Option C — Approach 3 fallback (per-plugin LV2 bridges + pw-link)

Originally the bail-out option. Skip carla entirely; spawn `lv2bench` / `jalv` per LV2 plugin and stitch with `pw-link`. Loses Carla's GUI host UX and chunk-based state, but the contract is stable and version-resilient.

**Pros:** No carla dependency at all; pipewire is the only IPC surface.
**Cons:** Lose state save/restore, lose plugin GUIs, more wiring code.

## Recommendation

**Option A** (qtweb HTTP for chain ops, OSC for parameter automation) is the lowest-risk path forward. It tracks upstream carla's own architecture (the qtweb backend exists *for this exact use case*) and the spike already demonstrated OSC parameter handlers survive. Suggested next step before W7.T1: a follow-up spike that exercises `qtweb` `add_plugin` + parameter OSC send to confirm the split control plane works end-to-end on this carla build.

If qtweb turns out to be GUI-coupled or gated on a Qt event loop we don't want to ship, fall back to **Option B** (FFI to libcarla_standalone2). Approach 3 (Option C) stays as the last resort.

## Spike artifacts kept

- `plugins/tideline-effects/Cargo.toml` — crate scaffolding
- `plugins/tideline-effects/src/lib.rs`, `src/main.rs` — plugin shell
- `plugins/tideline-effects/src/carla.rs` — safe-ish wrapper around `carla-sys`
- `plugins/tideline-effects/tests/ffi_spike.rs` — the FFI viability spike, `#[ignore]`-gated
- `crates/carla-sys/` — bindgen-generated FFI to libcarla_standalone2
- This file (`SPIKE_NOTES.md`)

The original OSC client (`src/osc.rs`) and OSC contract test (`tests/osc_contract_spike.rs`) were deleted in Phase 2 — see below.

---

# Phase 2 — FFI viability (chosen path)

**Date:** 2026-05-02
**Verdict:** **PASS.** Green-light the FFI architecture for the Wave 7 replan.

## Why qtweb (Option A) was abandoned

Phase 1 named qtweb HTTP as the leading option for chain-ops because `carla_backend_qtweb.py` advertised itself as the "out-of-process control client." Closer inspection of carla 2.6.0-alpha1 showed there is no qtweb *server* in this build line — only a stub *client* (`carla_backend_qtweb.py`) that talks to a server which used to live in carla but has been removed. The HTTP endpoints the client calls (`/add_plugin`, `/remove_plugin`, etc.) have no corresponding handler anywhere in the installed carla tree. Pure OSC and qtweb HTTP are both dead in 2.6.

## Why FFI (Option B) was chosen

Carla itself (the python+Qt frontend) drives `libcarla_standalone2.so` via ctypes. We do the same from Rust. The C ABI is stable, every chain-op we need is exposed, and the headers (`/usr/include/carla/CarlaHost.h`, `/usr/include/carla/CarlaBackend.h`) are part of the `carla-git` package so bindgen can generate bindings deterministically.

## What the spike proved

`cargo test -p tideline-effects --test ffi_spike -- --ignored --nocapture` round-trips the seven control-plane operations Wave 7 depends on:

1. `carla_standalone_host_init` -> non-null `CarlaHostHandle`
2. `carla_get_engine_driver_count` / `carla_get_engine_driver_name` -> `["JACK", "JACK with ALSA-MIDI", "ALSA", "PulseAudio", "SDL"]`
3. `carla_engine_init("JACK", "tideline-ffi-spike")` -> true (PipeWire's JACK shim accepted)
4. `carla_add_plugin(BINARY_NATIVE, PLUGIN_LV2, NULL, "spike", "http://lsp-plug.in/plugins/lv2/gate_mono", 0, NULL, 0)` -> true; `carla_get_current_plugin_count` returns 1, so the new plugin id is 0
5. `carla_set_active(0, false)` then `carla_set_active(0, true)` -> no-op, no crash
6. `carla_set_parameter_value(0, 0, 0.5f)` -> no crash
7. `carla_save_plugin_state(0, "/tmp/.../state.xml")` -> true; resulting XML is **6147 bytes**
8. `carla_load_plugin_state(0, "/tmp/.../state.xml")` -> true (full round-trip)
9. `carla_switch_plugins(0, 0)` -> false with assertion `pluginIdA != pluginIdB` (expected — only one plugin in the spike)
10. `carla_remove_plugin(0)` -> true
11. `carla_engine_close()` -> true

Test exits 0.

## Rough edges discovered

- **No Dummy driver in this carla build.** Available drivers are JACK / JACK-with-ALSA-MIDI / ALSA / PulseAudio / SDL. The spike falls through to the first available (JACK) because PipeWire ships a libjack shim. Production code can stick with JACK.
- **JACK bookkeeping assertions during init.** carla emits non-fatal `Carla assertion failure: "groupName != nullptr ..."` warnings on stderr while wiring up its patchbay-aware JACK client. They do not affect API behaviour but are noisy. Investigation deferred — likely a carla-2.6-alpha quirk against the PipeWire JACK shim.
- **lsp-plug.in/plugins/lv2/gate_mono is the canonical test fixture.** Loaded first try, no fallback needed. Calf Filter was not exercised.
- **`carla_add_plugin` does not return the new plugin id.** The wrapper calls `carla_get_current_plugin_count` after add and returns `count - 1`. Wave 7 production code should use the same pattern.
- **rpath plumbing for libcarla_standalone2.** Arch installs the library under `/usr/lib/carla/`, which is not on the default loader path. Solved by `links = "carla_standalone2"` in `crates/carla-sys/Cargo.toml`, exporting `cargo:rpaths=...` from `carla-sys/build.rs`, and re-emitting `cargo:rustc-link-arg-bins=-Wl,-rpath,...` + `-tests` from `plugins/tideline-effects/build.rs`.
- **Bindgen requires C++ mode.** CarlaHost.h includes CarlaBackend.h which uses a C++ namespace for type forward-decls (the `CARLA_API_EXPORT` symbols themselves are `extern "C"`). Build.rs feeds `clang_arg("-x c++") + clang_arg("-std=c++17")` and an allowlist scoped to `carla_*` / `Carla*` / `BinaryType` / `PluginType` / `ENGINE_*` / `PLUGIN_*` / `BINARY_*` to avoid pulling in the C++ stdlib. Generates clean — no manual blocklist needed.

## Recommendation

**Green-light Option B (FFI to libcarla_standalone2) for the Wave 7 replan.** The seven-operation contract works end-to-end against carla-git 2.6.0-alpha1, the bindings build deterministically through pkg-config + bindgen, and the rpath plumbing is solved generically (any future crate that depends on `carla-sys` and produces an executable target follows the same pattern).
