// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const METHOD_INITIALIZE: &str = "initialize";
pub const METHOD_NOTIFICATIONS_INITIALIZED: &str = "notifications/initialized";
pub const METHOD_NOTIFICATIONS_CANCELLED: &str = "notifications/cancelled";
pub const METHOD_PING: &str = "ping";
pub const METHOD_TOOLS_LIST: &str = "tools/list";
pub const METHOD_TOOLS_CALL: &str = "tools/call";

pub const MCP_ERROR_METHOD_NOT_FOUND: i32 = -32601;
pub const MCP_ERROR_INVALID_PARAMS: i32 = -32602;
pub const MCP_ERROR_INTERNAL: i32 = -32000;
pub const MCP_ERROR_AUTH_REQUIRED: i32 = -32001;
pub const MCP_ERROR_INVOCATION_CANCELLED: i32 = -32010;
pub const MCP_ERROR_UNKNOWN_EXECUTION_STATE: i32 = -32011;

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct CancelledNotificationParams {
    #[serde(rename = "requestId")]
    pub request_id: Value,
    #[serde(default)]
    pub reason: Option<String>,
}

impl CancelledNotificationParams {
    pub fn parse(params: Value) -> Result<Self, String> {
        let parsed = serde_json::from_value::<Self>(params)
            .map_err(|_| "notifications/cancelled.requestId is required".to_string())?;
        if !matches!(parsed.request_id, Value::String(_) | Value::Number(_)) {
            return Err("notifications/cancelled.requestId must be a string or number".to_string());
        }
        Ok(parsed)
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct JsonRpcRequest {
    #[allow(dead_code)]
    pub jsonrpc: Option<String>,
    pub id: Option<Value>,
    pub method: String,
    #[serde(default)]
    pub params: Value,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct JsonRpcResponse {
    pub jsonrpc: &'static str,
    pub id: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct JsonRpcError {
    pub code: i32,
    pub message: String,
}

pub fn jsonrpc_ok(id: Value, result: Value) -> JsonRpcResponse {
    JsonRpcResponse {
        jsonrpc: "2.0",
        id,
        result: Some(result),
        error: None,
    }
}

pub fn jsonrpc_error(id: Value, code: i32, message: impl Into<String>) -> JsonRpcResponse {
    JsonRpcResponse {
        jsonrpc: "2.0",
        id,
        result: None,
        error: Some(JsonRpcError {
            code,
            message: message.into(),
        }),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn cancelled_notification_accepts_string_or_number_request_ids() {
        assert_eq!(
            CancelledNotificationParams::parse(json!({"requestId": "call-1"}))
                .unwrap()
                .request_id,
            json!("call-1")
        );
        assert_eq!(
            CancelledNotificationParams::parse(json!({"requestId": 7}))
                .unwrap()
                .request_id,
            json!(7)
        );
        assert!(CancelledNotificationParams::parse(json!({"requestId": null})).is_err());
    }
}
