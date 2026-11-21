// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use serde_json::Value;

pub use crate::arguments::inject_agent_builder_args;
use crate::text_budget::tool_result_text_max_chars;
pub use crate::text_budget::{capability_description_budget_error, truncate_tool_text};
use crate::types::TransientToolModelInput;

const TRANSIENT_MODEL_INPUT_FIELD: &str = "__chatos_transient_model_input";
const MODEL_INPUT_FIELD: &str = "_model_input";
const MODEL_INPUT_MAX_IMAGES: usize = 2;
const MODEL_INPUT_MAX_DECODED_BYTES: usize = 2 * 1024 * 1024;
const DEDUPLICATED_STRUCTURED_TEXT_COPIES_FIELD: &str = "chatos/deduplicatedStructuredTextCopies";
const DEDUPLICATED_STRUCTURED_TEXT_PLACEHOLDER: &str =
    "[Duplicate JSON text omitted; use structuredContent as the canonical result.]";

pub fn to_text_and_structured_result(result: &Value) -> (String, Option<Value>) {
    to_text_and_structured_result_inner(result, false, tool_result_text_max_chars())
}

/// Project a failed external result into model-visible diagnostics, not its binary
/// attachments or internal traces. The original payload remains in the tool ledger.
pub(crate) fn external_tool_error_text(
    result: Option<&Value>,
    error: &str,
    max_chars: Option<usize>,
) -> String {
    let limit = max_chars.unwrap_or_else(tool_result_text_max_chars).max(1);
    let detail = result
        .and_then(|value| diagnostic_payload(value, 0))
        .map(|value| diagnostic_text(&value))
        .filter(|text| !text.trim().is_empty());
    let error = error.trim();
    let content = match (error.is_empty(), detail) {
        (true, Some(detail)) => detail,
        (false, Some(detail)) if detail.trim() != error => format!("{error}\n{detail}"),
        (false, _) => error.to_owned(),
        (true, None) => "tool execution failed".to_owned(),
    };
    truncate_tool_text(&content, limit)
}

fn diagnostic_payload(value: &Value, depth: usize) -> Option<Value> {
    if depth >= 16 || value.is_null() {
        return None;
    }
    match value {
        Value::Object(object) => {
            if object
                .get("type")
                .and_then(Value::as_str)
                .is_some_and(|kind| {
                    matches!(
                        kind,
                        "image"
                            | "audio"
                            | "resource"
                            | "resource_link"
                            | "input_image"
                            | "input_audio"
                    )
                })
            {
                return None;
            }
            let fields: serde_json::Map<String, Value> = object
                .iter()
                .filter_map(|(key, value)| {
                    if matches!(
                        key.as_str(),
                        "data"
                            | "blob"
                            | "base64"
                            | "image_url"
                            | "audio_url"
                            | "_model_input"
                            | TRANSIENT_MODEL_INPUT_FIELD
                            | "trace"
                            | "stack"
                            | "stackTrace"
                            | "stack_trace"
                            | "backtrace"
                    ) {
                        return None;
                    }
                    diagnostic_payload(value, depth + 1).map(|value| (key.clone(), value))
                })
                .collect();
            (!fields.is_empty()).then_some(Value::Object(fields))
        }
        Value::Array(values) => {
            let values: Vec<Value> = values
                .iter()
                .filter_map(|value| diagnostic_payload(value, depth + 1))
                .collect();
            (!values.is_empty()).then_some(Value::Array(values))
        }
        _ => Some(value.clone()),
    }
}

fn diagnostic_text(value: &Value) -> String {
    if let Some(text) = value.as_str() {
        return text.to_owned();
    }
    if let Some(payload) = deduplicated_model_payload(value) {
        return payload.to_string();
    }
    if let Some(content) = value.get("content") {
        if let Some(items) = content.as_array() {
            let text = items
                .iter()
                .filter_map(|item| {
                    (item.get("type").and_then(Value::as_str) == Some("text"))
                        .then(|| {
                            item.get("text")
                                .or_else(|| item.get("value"))
                                .and_then(Value::as_str)
                        })
                        .flatten()
                })
                .collect::<Vec<_>>()
                .join("\n");
            if !text.trim().is_empty() {
                return text;
            }
        } else {
            return diagnostic_text(content);
        }
    }
    for key in ["text", "value", "_structured_result", "structuredContent"] {
        if let Some(payload) = value.get(key) {
            return diagnostic_text(payload);
        }
    }
    value.to_string()
}

pub fn to_text_and_structured_result_with_transient(result: &Value) -> (String, Option<Value>) {
    to_text_and_structured_result_inner(result, true, tool_result_text_max_chars())
}

pub fn to_text_and_structured_result_with_transient_limit(
    result: &Value,
    max_chars: usize,
) -> (String, Option<Value>) {
    to_text_and_structured_result_inner(result, true, max_chars.max(1))
}

/// Return the innermost MCP structured payload.
///
/// Providers and transports may independently wrap a tool result in
/// `_structured_result`. Keeping the unwrapping here prevents the synchronous,
/// asynchronous and recovery paths from interpreting the same result
/// differently.
pub fn structured_result_payload(mut result: &Value) -> &Value {
    while let Some(payload) = result.get("_structured_result") {
        result = payload;
    }
    result
}

pub(crate) fn take_transient_model_input(
    structured_result: &mut Option<Value>,
) -> Option<TransientToolModelInput> {
    let structured = structured_result.as_mut()?.as_object_mut()?;
    let value = structured.remove(TRANSIENT_MODEL_INPUT_FIELD)?;
    let items = value.as_array()?.clone();
    (!items.is_empty()).then(|| TransientToolModelInput::new(items))
}

fn to_text_and_structured_result_inner(
    result: &Value,
    include_transient: bool,
    max_chars: usize,
) -> (String, Option<Value>) {
    let nested_mcp_result = result.get("content").filter(|value| value.is_object());
    let mut structured_result = result
        .get("_structured_result")
        .map(structured_result_payload)
        .cloned()
        .or_else(|| {
            nested_mcp_result
                .and_then(|value| value.get("structuredContent"))
                .cloned()
        });
    if include_transient {
        let model_input = validated_model_input_images(result.get(MODEL_INPUT_FIELD))
            .or_else(|| validated_mcp_content_images(result.get("content")))
            .or_else(|| {
                nested_mcp_result
                    .and_then(|value| validated_mcp_content_images(value.get("content")))
            })
            .or_else(|| {
                let payload = structured_result_payload(result);
                (!std::ptr::eq(payload, result))
                    .then(|| validated_mcp_content_images(payload.get("content")))
                    .flatten()
            });
        if let Some(items) = model_input {
            let structured =
                structured_result.get_or_insert_with(|| Value::Object(Default::default()));
            if let Some(map) = structured.as_object_mut() {
                map.insert(TRANSIENT_MODEL_INPUT_FIELD.to_string(), Value::Array(items));
            }
        }
    }
    let raw = if let Some(payload) = deduplicated_model_payload(result) {
        payload.to_string()
    } else if let Some(text) = result.as_str() {
        text.to_string()
    } else if let Some(content) = result.get("content").and_then(Value::as_array) {
        first_mcp_text(content).unwrap_or_else(|| result.to_string())
    } else if let Some(content) = nested_mcp_result
        .and_then(|value| value.get("content"))
        .and_then(Value::as_array)
    {
        first_mcp_text(content).unwrap_or_else(|| result.to_string())
    } else if let Some(text) = result.get("text").and_then(Value::as_str) {
        text.to_string()
    } else if let Some(value) = result.get("value").and_then(Value::as_str) {
        value.to_string()
    } else {
        result.to_string()
    };

    (
        truncate_tool_text(raw.as_str(), max_chars),
        structured_result,
    )
}

/// The native plugin bridge may remove an MCP text block when it is an exact
/// JSON copy of `structuredContent`. The model transport, however, consumes the
/// projected text rather than the sibling structured field. Restore that text
/// at the shared projection boundary so callers receive either readable JSON or
/// the Host's durable large-result reference, never a deduplication placeholder.
fn deduplicated_model_payload(value: &Value) -> Option<Value> {
    let envelope = value
        .get("content")
        .filter(|content| content.is_object())
        .unwrap_or(value);
    let removed_count = envelope
        .get("_meta")?
        .get(DEDUPLICATED_STRUCTURED_TEXT_COPIES_FIELD)?
        .as_u64()?;
    if removed_count == 0 {
        return None;
    }
    let structured = envelope.get("structuredContent")?.clone();
    let supplemental_text: Vec<Value> = envelope
        .get("content")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|item| item.get("type").and_then(Value::as_str) == Some("text"))
        .filter(|item| {
            item.get("text").and_then(Value::as_str)
                != Some(DEDUPLICATED_STRUCTURED_TEXT_PLACEHOLDER)
        })
        .cloned()
        .collect();
    let mut payload = serde_json::Map::from_iter([
        (
            "_meta".to_string(),
            serde_json::json!({DEDUPLICATED_STRUCTURED_TEXT_COPIES_FIELD: removed_count}),
        ),
        ("structuredContent".to_string(), structured),
    ]);
    if !supplemental_text.is_empty() {
        payload.insert("content".to_string(), Value::Array(supplemental_text));
    }
    Some(Value::Object(payload))
}

pub(crate) fn is_deduplicated_model_payload(value: &Value) -> bool {
    value
        .get("_meta")
        .and_then(|metadata| metadata.get(DEDUPLICATED_STRUCTURED_TEXT_COPIES_FIELD))
        .and_then(Value::as_u64)
        .is_some_and(|count| count > 0)
        && value.get("structuredContent").is_some()
}

fn first_mcp_text(content: &[Value]) -> Option<String> {
    content.iter().find_map(|item| {
        if item.get("type").and_then(Value::as_str) != Some("text") {
            return None;
        }
        item.get("text")
            .or_else(|| item.get("value"))
            .and_then(Value::as_str)
            .map(ToOwned::to_owned)
    })
}

fn validated_model_input_images(value: Option<&Value>) -> Option<Vec<Value>> {
    let values = value?.as_array()?;
    if values.is_empty() || values.len() > MODEL_INPUT_MAX_IMAGES {
        return None;
    }
    let mut decoded_bytes = 0usize;
    let mut images = Vec::with_capacity(values.len());
    for value in values {
        let image_url = value.get("image_url").and_then(Value::as_str)?;
        let encoded = [
            "data:image/png;base64,",
            "data:image/jpeg;base64,",
            "data:image/webp;base64,",
        ]
        .iter()
        .find_map(|prefix| image_url.strip_prefix(prefix))?;
        let bytes = STANDARD.decode(encoded).ok()?;
        decoded_bytes = decoded_bytes.checked_add(bytes.len())?;
        if decoded_bytes > MODEL_INPUT_MAX_DECODED_BYTES {
            return None;
        }
        let detail = value
            .get("detail")
            .and_then(Value::as_str)
            .filter(|detail| matches!(*detail, "auto" | "low" | "high"))
            .unwrap_or("high");
        images.push(serde_json::json!({
            "type": "input_image",
            "image_url": image_url,
            "detail": detail,
        }));
    }
    Some(images)
}

fn validated_mcp_content_images(value: Option<&Value>) -> Option<Vec<Value>> {
    let values = value?.as_array()?;
    let mut decoded_bytes = 0usize;
    let mut images = Vec::new();
    for value in values {
        if value.get("type").and_then(Value::as_str) != Some("image") {
            continue;
        }
        if images.len() >= MODEL_INPUT_MAX_IMAGES {
            return None;
        }
        let mime_type = value
            .get("mimeType")
            .or_else(|| value.get("mime_type"))
            .or_else(|| value.get("mime"))
            .and_then(Value::as_str)?;
        if !matches!(mime_type, "image/png" | "image/jpeg" | "image/webp") {
            return None;
        }
        let encoded = value.get("data").and_then(Value::as_str)?;
        let bytes = STANDARD.decode(encoded).ok()?;
        decoded_bytes = decoded_bytes.checked_add(bytes.len())?;
        if decoded_bytes > MODEL_INPUT_MAX_DECODED_BYTES {
            return None;
        }
        images.push(serde_json::json!({
            "type": "input_image",
            "image_url": format!("data:{mime_type};base64,{encoded}"),
            "detail": "high",
        }));
    }
    (!images.is_empty()).then_some(images)
}

#[cfg(test)]
mod tests;
