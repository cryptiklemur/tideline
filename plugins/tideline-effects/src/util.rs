use uuid::Uuid;

/// JACK client names are limited to 32 chars in many setups. We use the
/// uuid's first 8 hex chars for compactness while staying unique across a
/// reasonable channel count.
pub fn carla_client_name(channel_uuid: Uuid) -> String {
    let hex = channel_uuid.simple().to_string();
    format!("tideline-fx-{}", &hex[..8])
}

pub fn fx_input_port(channel_uuid: Uuid, lr: char) -> String {
    format!("{}:in_{}", carla_client_name(channel_uuid), lr)
}

pub fn fx_output_port(channel_uuid: Uuid, lr: char) -> String {
    format!("{}:out_{}", carla_client_name(channel_uuid), lr)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn carla_client_name_is_short_and_stable() {
        let u = Uuid::parse_str("12345678-9abc-def0-1234-56789abcdef0").unwrap();
        assert_eq!(carla_client_name(u), "tideline-fx-12345678");
    }

    #[test]
    fn input_and_output_ports_format_correctly() {
        let u = Uuid::parse_str("12345678-9abc-def0-1234-56789abcdef0").unwrap();
        assert_eq!(fx_input_port(u, 'l'), "tideline-fx-12345678:in_l");
        assert_eq!(fx_output_port(u, 'r'), "tideline-fx-12345678:out_r");
    }
}
