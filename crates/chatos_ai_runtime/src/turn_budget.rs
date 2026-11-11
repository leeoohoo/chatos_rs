// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde::ser::{SerializeMap, SerializeSeq};
use serde::{Serialize, Serializer};
use serde_json::Value;

use crate::traits::ModelRequest;

/// Image URLs carry pixels, not text tokens. Keep the original request intact and
/// omit only image-part payloads from this text/structure precheck. Vision usage
/// is model-specific and provider-owned; wire bytes retain their transport limit.
struct TextBudgetInput<'a>(&'a Value);

impl Serialize for TextBudgetInput<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self.0 {
            Value::Array(values) => {
                let mut sequence = serializer.serialize_seq(Some(values.len()))?;
                for value in values {
                    sequence.serialize_element(&TextBudgetInput(value))?;
                }
                sequence.end()
            }
            Value::Object(values) => {
                let is_image = matches!(
                    values.get("type").and_then(Value::as_str),
                    Some("input_image" | "image_url")
                );
                let mut map = serializer.serialize_map(Some(values.len()))?;
                for (key, value) in values {
                    if is_image && key == "image_url" {
                        map.serialize_entry(key, "[image]")?;
                    } else {
                        map.serialize_entry(key, &TextBudgetInput(value))?;
                    }
                }
                map.end()
            }
            value => value.serialize(serializer),
        }
    }
}

pub(super) fn validate_complete_request_budget(request: &ModelRequest) -> Result<(), String> {
    if request.model_capability_revision.trim().is_empty()
        || request.protocol_overhead_estimator_id.trim().is_empty()
        || request.context_window_tokens < 2_048
        || request.output_reserve_tokens < 256
        || request.output_reserve_tokens >= request.context_window_tokens
    {
        return Err("published model capability snapshot is required".to_string());
    }
    let input_bytes = serde_json::to_vec(&TextBudgetInput(&request.input))
        .map_err(|error| format!("serialize model input for budget precheck failed: {error}"))?
        .len();
    let tool_bytes = serde_json::to_vec(&request.tools)
        .map_err(|error| format!("serialize model tools for budget precheck failed: {error}"))?
        .len();
    let instruction_bytes = request.instructions.as_deref().map_or(0, str::len);
    let protocol_overhead_tokens = 256usize
        .saturating_add(request.tools.len().saturating_mul(64))
        .saturating_add(
            request
                .input
                .as_array()
                .map_or(1, Vec::len)
                .saturating_mul(16),
        );
    let estimated_input_tokens = input_bytes
        .saturating_add(tool_bytes)
        .saturating_add(instruction_bytes)
        .saturating_add(2)
        / 3
        + protocol_overhead_tokens;
    if estimated_input_tokens.saturating_add(request.output_reserve_tokens)
        > request.context_window_tokens
    {
        return Err(format!(
            "complete model request exceeds frozen context budget: estimated_input_tokens={estimated_input_tokens}, output_reserve_tokens={}, context_window_tokens={}",
            request.output_reserve_tokens, request.context_window_tokens
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn request(input: Value) -> ModelRequest {
        ModelRequest {
            input,
            model_capability_revision: "test".into(),
            context_window_tokens: 200_000,
            output_reserve_tokens: 30_000,
            protocol_overhead_estimator_id: "responses-json-v1".into(),
            ..ModelRequest::openai_compatible(
                "http://127.0.0.1",
                "test",
                "test",
                "test",
                Value::Null,
            )
        }
    }

    #[test]
    fn image_payload_is_not_text_and_original_pixels_are_unchanged() {
        let image_url = format!("data:image/png;base64,{}", "A".repeat(1_770_568));
        for part in [
            json!({"type":"input_image","image_url":image_url,"detail":"high"}),
            json!({"type":"image_url","image_url":{"url":image_url,"detail":"high"}}),
        ] {
            let request = request(json!([{"role":"user","content":[
                {"type":"input_text","text":"Describe this image"}, part
            ]}]));
            let original = request.input.clone();
            validate_complete_request_budget(&request).unwrap();
            assert_eq!(request.input, original);
        }
    }

    #[test]
    fn oversized_text_tools_and_instructions_still_fail_with_an_image() {
        let image = json!({"type":"input_image","image_url":"data:image/png;base64,AAAA"});
        let large_text = "x".repeat(600_000);
        let mut text = request(json!([{"role":"user","content":[
            image, {"type":"input_text","text":large_text}
        ]}]));
        assert!(validate_complete_request_budget(&text).is_err());
        text.input = json!([{"role":"user","content":[image]}]);
        text.instructions = Some(large_text.clone());
        assert!(validate_complete_request_budget(&text).is_err());
        text.instructions = None;
        text.tools = vec![json!({"type":"function","description":large_text})];
        assert!(validate_complete_request_budget(&text).is_err());
    }

    #[test]
    fn image_url_text_in_a_tool_result_is_still_counted() {
        let output = json!({"type":"input_image","image_url":"A".repeat(600_000)}).to_string();
        let request =
            request(json!([{"type":"function_call_output","call_id":"call","output":output}]));
        assert!(validate_complete_request_budget(&request).is_err());
    }
}
