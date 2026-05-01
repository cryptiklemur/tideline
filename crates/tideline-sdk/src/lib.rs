//! Tideline plugin SDK — shared types, RPC framing, plugin entry helpers.

pub mod types;
pub use types::Capability;

#[cfg(test)]
mod tests {
    #[test]
    fn sdk_links() {
        assert_eq!(2 + 2, 4);
    }
}
