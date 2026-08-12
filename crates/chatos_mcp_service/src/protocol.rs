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
pub const MCP_ERROR_CAPACITY_EXHAUSTED: i32 = -32012;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct McpToolCallCommand {
    pub batch_id: String,
    pub runtime_token: String,
    pub reply_to: String,
    pub calls: Vec<McpToolCallCommandItem>,
    #[serde(default = "initial_delivery_attempt")]
    pub delivery_attempt: u32,
}

impl McpToolCallCommand {
    pub fn normalize_delivery_attempt(mut self) -> Self {
        self.delivery_attempt = self.delivery_attempt.max(1);
        self
    }

    pub fn next_retry(&self, max_delivery_attempts: u32) -> Option<Self> {
        if self.delivery_attempt.max(1) >= max_delivery_attempts {
            return None;
        }
        let mut retry = self.clone();
        retry.delivery_attempt = self.delivery_attempt.max(1).saturating_add(1);
        Some(retry)
    }
}

fn initial_delivery_attempt() -> u32 {
    1
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct McpToolCallCommandItem {
    pub invocation_id: String,
    pub tool_call_id: String,
    pub call_index: usize,
    pub name: String,
    pub arguments: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preflight_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct McpToolCallResult {
    pub event_id: String,
    pub batch_id: String,
    pub session_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_id: Option<String>,
    pub items: Vec<McpToolCallResultItem>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum McpToolCallResultStatus {
    Completed,
    Failed,
    Cancelled,
    UnknownExecutionState,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct McpToolCallResultItem {
    pub invocation_id: String,
    pub tool_call_id: String,
    pub call_index: usize,
    pub name: String,
    pub status: McpToolCallResultStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_code: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

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
