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
    let mut structured_result = result
        .get("_structured_result")
        .map(structured_result_payload)
        .cloned();
    if include_transient {
        let model_input = validated_model_input_images(result.get(MODEL_INPUT_FIELD))
            .or_else(|| validated_mcp_content_images(result.get("content")))
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
        content
            .iter()
            .find_map(|item| {
                if item.get("type").and_then(Value::as_str) != Some("text") {
                    return None;
                }
                item.get("text")
                    .or_else(|| item.get("value"))
                    .and_then(Value::as_str)
                    .map(ToOwned::to_owned)
            })
            .unwrap_or_else(|| result.to_string())
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
        .unwrap_or(16_000)
}

pub fn truncate_tool_text(text: &str, max_chars: usize) -> String {
    if max_chars == 0 {
        return String::new();
    }
    let total = text.chars().count();
    if total <= max_chars {
        return text.to_string();
    }

    let marker = format!("\n...[truncated {} chars]...\n", total - max_chars);
    let marker_chars = marker.chars().count();
    if marker_chars >= max_chars {
        return text.chars().take(max_chars).collect();
    }

    let head_chars = ((max_chars - marker_chars) * 3 / 5).max(1);
    let tail_chars = (max_chars - marker_chars).saturating_sub(head_chars);
    let head: String = text.chars().take(head_chars).collect();
    let tail: String = text
        .chars()
        .rev()
        .take(tail_chars)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    format!("{}{}{}", head, marker, tail)
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
