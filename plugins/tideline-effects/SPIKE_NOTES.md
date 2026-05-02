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
- `plugins/tideline-effects/src/lib.rs`, `src/main.rs`, `src/osc.rs` — minimal OSC client
- `plugins/tideline-effects/tests/osc_contract_spike.rs` — the (weak-pass) test, marked `#[ignore]`
- This file (`SPIKE_NOTES.md`)

The OSC client and crate skeleton are still useful regardless of which option we pick — under Option A the client becomes the parameter-automation surface, under Option B it goes away (replaced by FFI), under Option C it goes away.
