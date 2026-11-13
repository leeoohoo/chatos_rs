// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde::ser::{SerializeMap, SerializeSeq};
use serde::{Serialize, Serializer};
use serde_json::Value;

use crate::memory_context::MemoryScope;
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

fn estimated_input_tokens(request: &ModelRequest) -> Result<usize, String> {
    if request.model_capability_revision.trim().is_empty()
        || request.protocol_overhead_estimator_id.trim().is_empty()
        || request.context_window_tokens == 0
        || request.output_reserve_tokens == 0
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
    Ok(input_bytes
        .saturating_add(tool_bytes)
        .saturating_add(instruction_bytes)
        .saturating_add(2)
        / 3
        + protocol_overhead_tokens)
}

pub(super) fn restrict_memory_scope(
    scope: &mut MemoryScope,
    baseline: &ModelRequest,
) -> Result<(), String> {
    if scope.model_capability_revision != baseline.model_capability_revision {
        return Err("Memory scope capability revision differs from the model request".into());
    }
    let fixed_input = estimated_input_tokens(baseline)?;
    let remaining = baseline
        .context_window_tokens
        .checked_sub(baseline.output_reserve_tokens)
        .and_then(|input| input.checked_sub(fixed_input))
        .filter(|remaining| *remaining > 0)
        .ok_or_else(|| {
            "current model request leaves no input capacity for Memory context".to_string()
        })?;
    let policy = scope.policy.as_mut().ok_or_else(|| {
        "Memory compose requires an explicit model-derived request budget".to_string()
    })?;
    policy.max_memory_tokens = policy.max_memory_tokens.min(remaining);
    policy.max_recent_record_tokens = policy
        .max_recent_record_tokens
        .min(policy.max_memory_tokens);
    policy.max_thread_summary_tokens = policy
        .max_thread_summary_tokens
        .min(policy.max_memory_tokens);
    policy.max_project_memory_tokens = policy
        .max_project_memory_tokens
        .min(policy.max_memory_tokens);
    policy.max_agent_memory_tokens = policy.max_agent_memory_tokens.min(policy.max_memory_tokens);
    Ok(())
}

pub(super) fn validate_complete_request_budget(request: &ModelRequest) -> Result<(), String> {
    let estimated_input_tokens = estimated_input_tokens(request)?;
    if request
        .max_output_tokens
        .is_some_and(|output| output <= 0 || output as u128 > request.output_reserve_tokens as u128)
    {
        return Err("max_output_tokens must not exceed the published output reserve".into());
    }
    // Responses compaction runs at the published soft input limit and makes
    // room for the reserved output before inference. The client must still
    // reject a raw request that already exceeds the real model window, but it
    // must not prevent the provider from receiving a compactable request.
    let input_limit =
        if request.supports_responses && request.responses_compaction_threshold.is_some() {
            request.context_window_tokens
        } else {
            request
                .context_window_tokens
                .saturating_sub(request.output_reserve_tokens)
        };
    if estimated_input_tokens > input_limit {
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
            context_window_tokens: 117_031,
            output_reserve_tokens: 17_031,
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

    #[test]
    fn memory_capacity_deducts_current_input_instructions_and_tools() {
        use memory_engine_sdk::ComposeContextPolicy;
        let mut baseline = request(json!([{"role":"user","content":"current request"}]));
        baseline.instructions = Some("instructions".repeat(100));
        baseline.tools = vec![json!({"name":"example","description":"tool schema".repeat(100)})];
        let mut scope = MemoryScope::thread("tenant", "source", "thread")
            .with_compose_context(None, None, "test")
            .with_policy(ComposeContextPolicy {
                max_memory_tokens: 100_000,
                max_recent_record_tokens: 100_000,
                max_thread_summary_tokens: 100_000,
                max_project_memory_tokens: 100_000,
                max_agent_memory_tokens: 100_000,
            });
        restrict_memory_scope(&mut scope, &baseline).unwrap();
        let expected = 100_000 - estimated_input_tokens(&baseline).unwrap();
        assert_eq!(scope.policy.as_ref().unwrap().max_memory_tokens, expected);
        assert_eq!(
            scope.policy.as_ref().unwrap().max_recent_record_tokens,
            expected
        );
        let first = expected;
        baseline.instructions = Some("x".repeat(90_000));
        restrict_memory_scope(&mut scope, &baseline).unwrap();
        assert!(scope.policy.as_ref().unwrap().max_memory_tokens < first);
        baseline.input = json!("x".repeat(400_000));
        assert!(restrict_memory_scope(&mut scope, &baseline).is_err());
        scope.model_capability_revision = "other".into();
        assert!(restrict_memory_scope(&mut scope, &baseline).is_err());
    }

    #[test]
    fn responses_compaction_allows_input_between_soft_and_hard_limits() {
        let mut compactable = request(json!("x".repeat(320_000)));
        compactable.supports_responses = true;
        compactable.responses_compaction_threshold =
            Some(compactable.context_window_tokens - compactable.output_reserve_tokens);
        let estimated = estimated_input_tokens(&compactable).unwrap();
        assert!(estimated > compactable.context_window_tokens - compactable.output_reserve_tokens);
        assert!(estimated <= compactable.context_window_tokens);
        validate_complete_request_budget(&compactable).unwrap();

        compactable.responses_compaction_threshold = None;
        assert!(validate_complete_request_budget(&compactable).is_err());
    }

    #[test]
    fn responses_compaction_still_rejects_input_above_the_hard_window() {
        let mut oversized = request(json!("x".repeat(400_000)));
        oversized.supports_responses = true;
        oversized.responses_compaction_threshold =
            Some(oversized.context_window_tokens - oversized.output_reserve_tokens);
        assert!(estimated_input_tokens(&oversized).unwrap() > oversized.context_window_tokens);
        assert!(validate_complete_request_budget(&oversized).is_err());
    }
}
