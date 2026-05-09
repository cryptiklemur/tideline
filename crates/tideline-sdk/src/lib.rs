//! Tideline Plugin SDK
//!
//! Re-exports framing, rpc, types, transport, client, and plugin trait for plugin authors.

pub mod client;
pub mod contribute;
pub mod framing;
pub mod logging;
pub mod plugin;
pub mod rpc;
pub mod transport;
pub mod types;
pub mod ui;

pub use client::HostClient;
pub use plugin::{run, Plugin};
pub use types::Capability;

#[cfg(test)]
mod tests {
    #[test]
    fn sdk_links() {
        assert_eq!(2 + 2, 4);
    }
}
