use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Id {
    Number(i64),
    String(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Request {
    pub jsonrpc: String,
    pub id: Id,
    pub method: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub params: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Response {
    pub jsonrpc: String,
    pub id: Id,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<RpcError>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Notification {
    pub jsonrpc: String,
    pub method: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub params: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Message {
    Request(Request),
    Response(Response),
    Notification(Notification),
}

#[derive(Debug, Clone, Serialize, Deserialize, thiserror::Error)]
#[error("rpc error {code}: {message}")]
pub struct RpcError {
    pub code: i32,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

pub mod error_codes {
    pub const PARSE_ERROR: i32 = -32700;
    pub const INVALID_REQUEST: i32 = -32600;
    pub const METHOD_NOT_FOUND: i32 = -32601;
    pub const INVALID_PARAMS: i32 = -32602;
    pub const INTERNAL_ERROR: i32 = -32603;
    pub const CAPABILITY_DENIED: i32 = -32001;
    pub const TOPIC_UNDECLARED: i32 = -32002;
    pub const RATE_LIMITED: i32 = -32003;
    pub const HANDSHAKE_FAILED: i32 = -32004;
}

impl Request {
    pub fn new(id: Id, method: impl Into<String>, params: Option<Value>) -> Self {
        Self { jsonrpc: "2.0".into(), id, method: method.into(), params }
    }
}

impl Response {
    pub fn ok(id: Id, result: Value) -> Self {
        Self { jsonrpc: "2.0".into(), id, result: Some(result), error: None }
    }

    pub fn err(id: Id, error: RpcError) -> Self {
        Self { jsonrpc: "2.0".into(), id, result: None, error: Some(error) }
    }
}

impl Notification {
    pub fn new(method: impl Into<String>, params: Option<Value>) -> Self {
        Self { jsonrpc: "2.0".into(), method: method.into(), params }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn untagged_message_distinguishes_request_response_notification() {
        let req = serde_json::json!({"jsonrpc":"2.0","id":1,"method":"host/initialize"});
        let msg: Message = serde_json::from_value(req).unwrap();
        assert!(matches!(msg, Message::Request(_)));

        let res = serde_json::json!({"jsonrpc":"2.0","id":1,"result":{}});
        let msg: Message = serde_json::from_value(res).unwrap();
        assert!(matches!(msg, Message::Response(_)));

        let note = serde_json::json!({"jsonrpc":"2.0","method":"host/event.fire","params":{}});
        let msg: Message = serde_json::from_value(note).unwrap();
        assert!(matches!(msg, Message::Notification(_)));
    }

    #[test]
    fn capability_denied_code_is_minus_32001() {
        assert_eq!(error_codes::CAPABILITY_DENIED, -32001);
    }
}
