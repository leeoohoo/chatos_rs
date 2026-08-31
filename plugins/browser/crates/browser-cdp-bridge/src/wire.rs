use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const PROTOCOL_VERSION: &str = "1.0";
pub const BROWSER_SUBPROTOCOL: &str = "chatos-browser-bridge.v1";
pub const EXTENSION_SUBPROTOCOL: &str = "chatos-browser-extension.v1";
pub const CONTROL_SUBPROTOCOL: &str = "chatos-browser-control.v1";
pub const MAX_MESSAGE_BYTES: usize = 8 * 1024 * 1024;

#[derive(Debug, Deserialize)]
pub struct WireMessage {
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default)]
    pub id: Option<Value>,
    #[serde(default)]
    pub method: Option<String>,
    #[serde(default)]
    pub params: Value,
    #[serde(default)]
    pub result: Option<Value>,
    #[serde(default)]
    pub error: Option<WireError>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WireError {
    pub code: String,
    pub message: String,
}

impl WireError {
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into().chars().take(1024).collect(),
        }
    }
}

pub fn response(id: Value, result: Value) -> Value {
    serde_json::json!({"type":"response", "id":id, "result":result})
}

pub fn error_response(id: Value, error: WireError) -> Value {
    serde_json::json!({"type":"response", "id":id, "error":error})
}

pub fn event(method: &str, params: Value) -> Value {
    serde_json::json!({"type":"event", "method":method, "params":params})
}

pub fn request(id: u64, method: &str, params: Value) -> Value {
    serde_json::json!({"type":"request", "id":id, "method":method, "params":params})
}
