// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

pub(crate) const MCP_RELAY_MESSAGE_TYPE: &str = "mcp";

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct RelayRequest {
    #[serde(rename = "type")]
    pub(crate) _message_type: String,
    pub(crate) request_id: String,
    #[allow(dead_code)]
    pub(crate) owner_user_id: Option<String>,
    #[allow(dead_code)]
    pub(crate) device_id: Option<String>,
    pub(crate) workspace_id: String,
    pub(crate) method: Option<String>,
    pub(crate) path: Option<String>,
    #[serde(default)]
    pub(crate) headers: BTreeMap<String, String>,
    #[serde(default)]
    pub(crate) body: Value,
}

#[derive(Debug, Serialize)]
pub(crate) struct RelayResponse {
    #[serde(rename = "type")]
    pub(crate) message_type: String,
    pub(crate) request_id: String,
    pub(crate) status: u16,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub(crate) headers: BTreeMap<String, String>,
    pub(crate) body: Value,
}

impl RelayResponse {
    pub(crate) fn to_value(self) -> Value {
        serde_json::to_value(self).unwrap_or_else(|err| {
            json!({
                "type": "relay_response",
                "request_id": "",
                "status": 500,
                "body": {"error": err.to_string()}
            })
        })
    }
}

pub(crate) fn relay_error_response(
    message_type: &str,
    request_id: &str,
    status: u16,
    message: String,
) -> Value {
    RelayResponse {
        message_type: message_type.to_string(),
        request_id: request_id.to_string(),
        status,
        headers: BTreeMap::new(),
        body: json!({ "error": message }),
    }
    .to_value()
}

pub(crate) fn terminal_event(message_type: &str, terminal_session_id: &str, body: Value) -> Value {
    json!({
        "type": message_type,
        "terminal_session_id": terminal_session_id,
        "body": body,
    })
}
