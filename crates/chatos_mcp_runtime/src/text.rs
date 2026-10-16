// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use serde_json::Value;

use crate::types::TransientToolModelInput;

const TRANSIENT_MODEL_INPUT_FIELD: &str = "__chatos_transient_model_input";
const MODEL_INPUT_FIELD: &str = "_model_input";
const MODEL_INPUT_MAX_IMAGES: usize = 2;
const MODEL_INPUT_MAX_DECODED_BYTES: usize = 2 * 1024 * 1024;

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
    let raw = if let Some(text) = result.as_str() {
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

pub fn inject_agent_builder_args(args: Value, caller_model: Option<&str>) -> Value {
    let Some(model_name) = caller_model
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return args;
    };

    let mut obj = match args {
        Value::Object(map) => map,
        Value::Null => serde_json::Map::new(),
        _ => return args,
    };

    obj.entry("caller_model".to_string())
        .or_insert_with(|| Value::String(model_name.to_string()));

    Value::Object(obj)
}

fn tool_result_text_max_chars() -> usize {
    std::env::var("MCP_TOOL_RESULT_MAX_CHARS")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(160_000)
}

pub fn truncate_tool_text(text: &str, max_chars: usize) -> String {
    if let Some(diagnostic) = capability_description_budget_error(text, max_chars) {
        return diagnostic;
    }
    if max_chars == 0 {
        return String::new();
    }
    let total = text.chars().count();
    if total <= max_chars {
        return text.to_string();
    }

    serde_json::json!({
        "error": "tool_result_budget_exceeded",
        "original_chars": total,
        "budget_chars": max_chars,
        "content_preserved": false,
        "guidance": "The tool result was not returned as a damaged head/tail excerpt. Use the tool's offset/limit pagination, a managed artifact, or the Host tool_result_read reference when available."
    })
    .to_string()
}

/// Machine-readable capability definitions must never become damaged JSON.
/// Small definitions pass through; over-budget definitions get a recoverable,
/// explicit diagnostic rather than a surviving unrelated head/tail tool list.
pub fn capability_description_budget_error(text: &str, max_chars: usize) -> Option<String> {
    if text.chars().count() <= max_chars {
        return None;
    }
    let value: Value = serde_json::from_str(text).ok()?;
    let descriptor = value
        .get("content")
        .filter(|v| v.is_object())
        .unwrap_or(&value);
    if descriptor
        .get("binding_schema_version")
        .and_then(Value::as_u64)
        != Some(1)
        && descriptor.get("disclosure_version").and_then(Value::as_u64) != Some(2)
    {
        return None;
    }
    let plugin = descriptor.get("plugin_option").and_then(Value::as_str)?;
    Some(serde_json::json!({
        "error":"capability_description_budget_exceeded",
        "plugin_option":plugin,
        "response_chars":text.chars().count(),
        "budget_chars":max_chars,
        "guidance":"No tool schema was truncated. Use capability_describe with query or smaller limit for an index, then a single tool_option. For a paged schema use its next_schema_offset. Follow only parameters advertised by this Run. If these fields are unavailable, create a new Run with the current capability policy; there is no fallback to full-catalog disclosure. Do not guess missing arguments or substitute unrelated raw tools."
    }).to_string())
}

#[cfg(test)]
mod capability_budget_tests {
    use super::*;

    #[test]
    fn machine_catalog_overflow_is_valid_explicit_error_not_a_broken_definition() {
        for wrapped in [false, true] {
            let descriptor = serde_json::json!({"binding_schema_version":1,"plugin_option":"plugin_1",
                "provider_id":"plugin:test","tools":[{"name":"browser_navigate",
                    "input_schema":{"description":"中😀".repeat(15_000)}}]});
            let value = if wrapped {
                serde_json::json!({"content":descriptor,"is_error":false})
            } else {
                descriptor
            };
            let output = truncate_tool_text(&value.to_string(), 16_000);
            let error: Value = serde_json::from_str(&output).expect("valid JSON diagnostic");
            assert_eq!(error["error"], "capability_description_budget_exceeded");
            assert_eq!(error["plugin_option"], "plugin_1");
            assert!(output.chars().count() < 16_000);
            assert!(!output.contains("...[truncated"));
        }
    }

    #[test]
    fn small_pages_are_preserved_and_unrelated_overflow_is_an_explicit_error() {
        let page = serde_json::json!({"disclosure_version":2,"plugin_option":"plugin_1",
            "view":"tool","tools":[{"input_schema":{"type":"object"}}]})
        .to_string();
        assert_eq!(truncate_tool_text(&page, 16_000), page);
        let overflow: Value =
            serde_json::from_str(&truncate_tool_text(&"x".repeat(20_000), 16_000)).unwrap();
        assert_eq!(overflow["error"], "tool_result_budget_exceeded");
        assert_eq!(overflow["original_chars"], 20_000);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn transient_images_are_validated_and_removed_from_structured_history() {
        let payload = json!({
            "text": "screenshot captured",
            "_structured_result": {"persisted": false, "sha256": "abc"},
            "_model_input": [{
                "type": "input_image",
                "image_url": "data:image/jpeg;base64,/9j/AA==",
                "detail": "high"
            }]
        });
        let (text, mut structured) = to_text_and_structured_result_with_transient(&payload);
        assert_eq!(text, "screenshot captured");
        let transient = take_transient_model_input(&mut structured).expect("transient image");
        assert_eq!(transient.items().len(), 1);
        assert!(!structured
            .expect("structured result")
            .to_string()
            .contains("base64"));
    }

    #[test]
    fn standard_mcp_image_content_becomes_transient_model_input() {
        let payload = json!({
            "content": [
                {"type": "text", "text": "App=md.obsidian"},
                {
                    "type": "image",
                    "data": "iVBORw0KGgo=",
                    "mimeType": "image/png"
                }
            ],
            "isError": false
        });

        let (text, mut structured) = to_text_and_structured_result_with_transient(&payload);
        assert_eq!(text, "App=md.obsidian");
        let transient = take_transient_model_input(&mut structured).expect("transient image");
        assert_eq!(
            transient.items()[0]["image_url"],
            "data:image/png;base64,iVBORw0KGgo="
        );
        assert!(!structured
            .unwrap_or(Value::Null)
            .to_string()
            .contains("iVBORw0KGgo="));
    }

    #[test]
    fn wrapped_mcp_result_exposes_text_structured_content_and_transient_image() {
        let payload = json!({
            "content": {
                "content": [
                    {"type": "text", "text": "Active application: ChatOS"},
                    {"type": "image", "data": "/9j/AA==", "mimeType": "image/jpeg"}
                ],
                "isError": false,
                "structuredContent": {
                    "activeApplication": {"name": "ChatOS"}
                }
            },
            "is_error": false,
            "made_progress": true
        });

        let (text, mut structured) = to_text_and_structured_result_with_transient(&payload);
        let transient = take_transient_model_input(&mut structured).expect("transient image");

        assert_eq!(text, "Active application: ChatOS");
        assert_eq!(
            structured
                .as_ref()
                .and_then(|value| value.pointer("/activeApplication/name"))
                .and_then(Value::as_str),
            Some("ChatOS")
        );
        assert_eq!(
            transient.items()[0]["image_url"],
            "data:image/jpeg;base64,/9j/AA=="
        );
    }

    #[test]
    fn remote_and_oversized_model_images_are_not_forwarded() {
        for image_url in [
            "https://example.com/screenshot.png".to_string(),
            format!(
                "data:image/png;base64,{}",
                STANDARD.encode(vec![0_u8; MODEL_INPUT_MAX_DECODED_BYTES + 1])
            ),
        ] {
            let payload = json!({
                "text": "capture",
                "_model_input": [{"image_url": image_url}]
            });
            let (_, mut structured) = to_text_and_structured_result_with_transient(&payload);
            assert!(take_transient_model_input(&mut structured).is_none());
        }
    }

    #[test]
    fn explicit_text_limit_overrides_the_legacy_environment_default() {
        let payload = json!({
            "content": [{"type": "text", "text": "x".repeat(20_000)}]
        });

        let (text, _) = to_text_and_structured_result_with_transient_limit(&payload, 40_000);

        assert_eq!(text.chars().count(), 20_000);
        assert!(!text.contains("...[truncated"));
    }

    #[test]
    fn nested_structured_result_wrappers_are_fully_unwrapped() {
        let payload = json!({
            "content": [{"type": "text", "text": "completed"}],
            "_structured_result": {
                "_structured_result": {
                    "_structured_result": {
                        "changed": true,
                        "sha256": "abc"
                    }
                }
            }
        });

        let (text, structured) = to_text_and_structured_result(&payload);

        assert_eq!(text, "completed");
        assert_eq!(structured, Some(json!({"changed": true, "sha256": "abc"})));
        assert_eq!(
            structured_result_payload(&payload),
            &json!({"changed": true, "sha256": "abc"})
        );
    }
}
