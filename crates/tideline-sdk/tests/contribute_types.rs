use serde_json::json;
use tideline_sdk::contribute::{PipewireContributeRequest, PipewireContributeResponse, SerializedAppConfig};

#[test]
fn request_roundtrips() {
    let req = PipewireContributeRequest {
        config: SerializedAppConfig { json: json!({"channels": []}) },
        mix_mutes: vec![],
    };
    let s = serde_json::to_string(&req).unwrap();
    let _back: PipewireContributeRequest = serde_json::from_str(&s).unwrap();
}

#[test]
fn empty_response_serializes() {
    let r = PipewireContributeResponse { directives: vec![] };
    let s = serde_json::to_string(&r).unwrap();
    let back: PipewireContributeResponse = serde_json::from_str(&s).unwrap();
    assert!(back.directives.is_empty());
}
