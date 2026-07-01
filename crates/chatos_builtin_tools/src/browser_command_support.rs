// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde_json::Value;

pub(crate) fn parse_browser_command_eval_payload(raw: Value) -> Value {
    if let Some(text) = raw.as_str() {
        serde_json::from_str::<Value>(text).unwrap_or_else(|_| Value::String(text.to_string()))
    } else {
        raw
    }
}

pub(crate) fn browser_command_succeeded(value: &Value) -> bool {
    value
        .get("success")
        .and_then(|value| value.as_bool())
        .unwrap_or(false)
}

pub(crate) fn browser_command_error_text(value: &Value, fallback: &str) -> String {
    value
        .get("error")
        .and_then(|value| value.as_str())
        .unwrap_or(fallback)
        .to_string()
}
