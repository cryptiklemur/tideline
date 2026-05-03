use base64::engine::general_purpose::STANDARD;
use base64::Engine;

pub fn encode_state(bytes: &[u8]) -> String {
    STANDARD.encode(bytes)
}

pub fn decode_state(s: &str) -> Option<Vec<u8>> {
    STANDARD.decode(s).ok()
}
