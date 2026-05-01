//! Tideline Plugin SDK
//!
//! Re-exports framing, rpc, types, transport, client, and plugin trait for plugin authors.

pub mod types;
pub mod rpc;
pub mod framing;
pub mod transport;
pub mod client;
pub mod plugin;
pub mod contribute;

pub use types::Capability;
pub use plugin::{Plugin, run};
pub use client::HostClient;

#[cfg(test)]
mod tests {
    #[test]
    fn sdk_links() {
        assert_eq!(2 + 2, 4);
    }
}
