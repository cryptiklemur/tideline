use tideline_effects::persist::{decode_state, encode_state};

#[test]
fn empty_state_round_trips() {
    let chunk = b"";
    let s = encode_state(chunk);
    assert_eq!(decode_state(&s).unwrap(), Vec::<u8>::new());
}

#[test]
fn arbitrary_state_round_trips() {
    let chunk = b"<carla-state-chunk>raw bytes \x00\x01\x02</>";
    let s = encode_state(chunk);
    assert_eq!(decode_state(&s).unwrap(), chunk.to_vec());
}

#[test]
fn invalid_b64_returns_none() {
    assert!(decode_state("not valid base64!@#$").is_none());
}
