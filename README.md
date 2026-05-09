# tideline

> ⚠️ **Heavy WIP.** Nothing here is stable. APIs, plugin ABI, config formats, and PipeWire wiring all change without warning. Don't depend on it yet.

An open-source, Linux-native take on something like Wave Link — PipeWire-based audio routing, mic FX, push-to-talk, and a plugin SDK. Built with Tauri + SvelteKit on the frontend and a Rust workspace under the hood.

## What works today

- **Channel routing**: physical inputs (mic, system, browser, game, music, chat, etc.) routed to per-app virtual sources/sinks via PipeWire loopback modules.
- **Mic FX rack**: in-process LV2 host (livi + suil) with X11UI window embedding. Drag/drop chain editor, per-slot bypass, persisted across restarts. Audition mode lets you record a short clip (or pick a wav) and loop it through the chain to A/B effects without talking.
- **Push-to-talk**: evdev or XDG portal capture for the keybind, with mute toggle, soft-mute (volume clamp), and notification/tone hooks.
- **Plugin SDK**: stdio JSON-RPC, contribution-based UI surfaces (settings sections, channel cards, racks), shared persisted config namespace per plugin. Bundled plugins (`tideline-ptt`, `tideline-tones`, `tideline-notifications`, `tideline-effects`) live in this repo as the reference implementation.

## What doesn't

- macOS / Windows. Linux + PipeWire only for now.
- Wayland-only sessions for the LV2 UI path. The X11UI embedding still goes through xcb, so XWayland is fine but a pure Wayland UI host doesn't exist yet.
- Plugin discovery for non-LV2 formats. Carla replacement landed for LV2; CLAP/VST3 are on the list.
- Stable plugin ABI. Don't ship third-party plugins against the SDK yet — the wire protocol is moving.

## Building

Requires PipeWire (with `pw-cat`, `pw-link`), a recent Rust toolchain, Node 20+, and pnpm.

```sh
pnpm install
pnpm tauri dev
```

`pnpm build:plugins` compiles the bundled plugins separately.

## Layout

- `src/` — SvelteKit frontend (channel UI, FX rack dialog, settings).
- `src-tauri/` — Tauri shell, IPC commands, plugin host glue.
- `crates/tideline-core` — shared domain types, PipeWire topology builder.
- `crates/tideline-host` — JSON-RPC plugin host runtime.
- `crates/tideline-sdk` — what plugins link against.
- `plugins/tideline-*` — bundled reference plugins.

## License

MIT. See `LICENSE` (forthcoming — drop a `LICENSE` file before depending on this).
