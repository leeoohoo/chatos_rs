// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde_json::{json, Value};

use crate::state::AppState;

pub(super) async fn drop_terminal_subscription(
    state: &AppState,
    terminal_session_id: &str,
    subscription_id: &str,
) {
    if let Err(error) = state
        .relay
        .drop_terminal_subscription(terminal_session_id, subscription_id)
        .await
    {
        tracing::warn!(
            terminal_session_id,
            subscription_id,
            error = error.as_str(),
            "drop Local Connector terminal subscriber lease failed"
        );
    }
}

pub(super) fn terminal_event_to_ws_payload(message_type: &str, body: &Value) -> Option<Value> {
    match message_type {
        "terminal_output" => Some(json!({
            "type": "output",
            "data": body.get("data").and_then(Value::as_str).unwrap_or_default(),
        })),
        "terminal_snapshot" => Some(json!({
            "type": "snapshot",
            "data": body.get("data").and_then(Value::as_str).unwrap_or_default(),
        })),
        "terminal_exit" => Some(json!({
            "type": "exit",
            "code": body.get("code").and_then(Value::as_i64).unwrap_or(0),
        })),
        "terminal_state" => Some(json!({
            "type": "state",
            "busy": body.get("busy").and_then(Value::as_bool).unwrap_or(false),
            "snapshot_paging": true,
        })),
        "terminal_error" => Some(json!({
            "type": "error",
            "error": body
                .get("error")
                .and_then(Value::as_str)
                .unwrap_or("Local Connector terminal error"),
        })),
        _ => None,
    }
}
