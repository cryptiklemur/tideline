use uuid::Uuid;

/// In-process audio engine exposes one JACK client per channel, named
/// `tideline-fx-{simple_uuid}` (see `engine::channel::Channel::open`).
/// The whole chain runs inside that single client, so callers no longer
/// need per-effect names — only this channel-level node identifier.
pub fn channel_jack_client(channel_uuid: Uuid) -> String {
    format!("tideline-fx-{}", channel_uuid.simple())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn channel_node_name_matches_engine() {
        let ch = Uuid::parse_str("8fc82af3-1dfc-41ce-8eb0-528e68852657").unwrap();
        assert_eq!(
            channel_jack_client(ch),
            "tideline-fx-8fc82af31dfc41ce8eb0528e68852657"
        );
    }
}
