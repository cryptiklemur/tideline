//! Tideline core: shared model, config IO, and PipeWire conf generation.
//!
//! GUI-free so the host, plugins, and tooling can use it without pulling in tauri.

pub mod binding;
pub mod config_io;
pub mod model;
pub mod pipewire;
