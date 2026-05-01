//! Tideline plugin host: registry, supervisor, transport, capability gate, event bus.

pub mod paths;
pub mod capabilities;
pub mod manifest;
pub mod events;
pub mod logging;
pub mod transport;
pub mod runtime;
pub mod supervisor;
pub mod install;
pub mod dispatcher;
pub mod registry;
pub mod rpc;
pub mod contribute;
pub mod contributions;
pub mod iframe;

pub use iframe::{IframeBridge, IframeMessage, PluginIframeIncoming};
pub use registry::{InstalledPlugin, PluginRegistry, RegistryError};
