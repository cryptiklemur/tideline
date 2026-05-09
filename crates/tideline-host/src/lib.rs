//! Tideline plugin host: registry, supervisor, transport, capability gate, event bus.

pub mod backend;
pub mod capabilities;
pub mod contribute;
pub mod contributions;
pub mod dispatcher;
pub mod events;
pub mod iframe;
pub mod install;
pub mod logging;
pub mod manifest;
pub mod paths;
pub mod registry;
pub mod rpc;
pub mod runtime;
pub mod supervisor;
pub mod transport;

pub use iframe::{IframeBridge, IframeMessage, PluginIframeIncoming};
pub use registry::{InstalledPlugin, PluginRegistry, RegistryError};
