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
    pub owner_service: String,
    pub agent_run_id: String,
    pub agent_key: String,
    pub ordering_lane_key: String,
    pub lane_seq: u64,
    pub generation: u64,
    pub source_step_seq: u64,
    pub batch_id: String,
    pub mcp_runtime_session_ref: String,
    pub result_routing_key: String,
    pub calls: Vec<McpToolCallCommandItem>,
    #[serde(default = "initial_delivery_attempt")]
    pub delivery_attempt: u32,
}

impl McpToolCallCommand {
    pub fn validate(&self) -> Result<(), String> {
        for (name, value) in [
            ("owner_service", self.owner_service.as_str()),
            ("agent_run_id", self.agent_run_id.as_str()),
            ("agent_key", self.agent_key.as_str()),
            ("ordering_lane_key", self.ordering_lane_key.as_str()),
            ("batch_id", self.batch_id.as_str()),
            (
                "mcp_runtime_session_ref",
                self.mcp_runtime_session_ref.as_str(),
            ),
            ("result_routing_key", self.result_routing_key.as_str()),
        ] {
            if value.trim().is_empty() {
                return Err(format!("MCP tool call command {name} must not be empty"));
            }
        }
        if self.lane_seq == 0 || self.generation == 0 || self.source_step_seq == 0 {
            return Err(
                "MCP tool call command lane_seq, generation and source_step_seq must be positive"
                    .to_string(),
            );
        }
        if self.calls.is_empty() || self.calls.len() > 128 {
            return Err("MCP tool call command must contain between 1 and 128 calls".to_string());
        }
        for (index, call) in self.calls.iter().enumerate() {
            if call.call_index != index {
                return Err("MCP tool call command call_index must match array order".to_string());
            }
        }
        Ok(())
    }

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
    pub owner_service: String,
    pub agent_run_id: String,
    pub agent_key: String,
    pub ordering_lane_key: String,
    pub lane_seq: u64,
    pub generation: u64,
    pub source_step_seq: u64,
    pub batch_id: String,
    pub session_id: String,
    pub items: Vec<McpToolCallResultItem>,
}

impl McpToolCallResult {
    pub fn validate(&self) -> Result<(), String> {
        for (name, value) in [
            ("event_id", self.event_id.as_str()),
            ("owner_service", self.owner_service.as_str()),
            ("agent_run_id", self.agent_run_id.as_str()),
            ("agent_key", self.agent_key.as_str()),
            ("ordering_lane_key", self.ordering_lane_key.as_str()),
            ("batch_id", self.batch_id.as_str()),
            ("session_id", self.session_id.as_str()),
        ] {
            if value.trim().is_empty() {
                return Err(format!("MCP tool call result {name} must not be empty"));
            }
        }
        if self.lane_seq == 0 || self.generation == 0 || self.source_step_seq == 0 {
            return Err(
                "MCP tool call result lane_seq, generation and source_step_seq must be positive"
                    .to_string(),
            );
        }
        if self.items.is_empty() || self.items.len() > 128 {
            return Err("MCP tool call result must contain between 1 and 128 items".to_string());
        }
        for (index, item) in self.items.iter().enumerate() {
            if item.call_index != index {
                return Err("MCP tool call result call_index must match array order".to_string());
            }
        }
        Ok(())
    }
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
