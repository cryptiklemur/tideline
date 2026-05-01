//! Tideline plugin host: registry, supervisor, transport, capability gate, event bus.

pub mod capabilities;
pub mod manifest;
pub mod paths;

#[cfg(test)]
mod tests {
    #[test]
    fn host_links() {
        assert_eq!(2 + 2, 4);
    }
}
