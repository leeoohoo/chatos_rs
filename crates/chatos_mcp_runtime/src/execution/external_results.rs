// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde_json::{json, Value};

use super::{tool_result_error, tool_result_success};
use crate::types::ToolResult;

pub fn external_tool_result_from_value(
    tool_call_id: String,
    name: String,
    conversation_turn_id: Option<String>,
    value: &Value,
    max_chars: Option<usize>,
) -> ToolResult {
    let is_error = ["isError", "is_error"]
        .iter()
        .any(|key| value.get(key).and_then(Value::as_bool) == Some(true));
    let (content, structured_result) = max_chars
        .map(|limit| crate::text::to_text_and_structured_result_with_transient_limit(value, limit))
        .unwrap_or_else(|| crate::text::to_text_and_structured_result_with_transient(value));
    let mut result = tool_result_success(
        tool_call_id,
        name,
        conversation_turn_id,
        content,
        structured_result,
    );
    if is_error {
        result.success = false;
        result.is_error = true;
        result.content = crate::text::external_tool_error_text(Some(value), "", max_chars);
        result.transient_model_input = None;
    }
    result
}

/// Returns the complete, deterministic payload addressed by a durable tool-result
/// reference. Plain-text tools keep their original text; tools with structured
/// output expose both the display text and structured result so callers can use
/// JSON Pointer queries without copying the full payload into model context.
pub fn external_tool_result_reference_content(result: &ToolResult) -> String {
    if let Ok(model_payload) = serde_json::from_str::<Value>(&result.content) {
        if crate::text::is_deduplicated_model_payload(&model_payload) {
            return result.content.clone();
        }
    }
    match result.result.as_ref().filter(|value| !value.is_null()) {
        Some(structured_result) => json!({
            "content": result.content,
            "structured_result": structured_result,
        })
        .to_string(),
        None => result.content.clone(),
    }
}

/// A durable failure status is authoritative even if the provider payload has no
/// error flag. Keep both the transport code and the actionable provider diagnosis.
pub fn external_tool_failure_from_value(
    tool_call_id: String,
    name: String,
    conversation_turn_id: Option<String>,
    value: Option<&Value>,
    error: &str,
    max_chars: Option<usize>,
) -> ToolResult {
    let mut result = tool_result_error(
        tool_call_id,
        name,
        conversation_turn_id,
        crate::text::external_tool_error_text(value, error, max_chars),
        false,
    );
    result.result = value.filter(|value| !value.is_null()).cloned();
    result
}
