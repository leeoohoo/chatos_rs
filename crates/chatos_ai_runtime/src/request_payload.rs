// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde_json::{json, Value};

use crate::input_transform::normalize_responses_request_input;
use crate::model_config::{
    normalize_provider, normalize_thinking_level, reasoning_effort_for_provider,
};
use crate::{AiRequestOptions, JsonSchemaOutputFormat};

pub const CHAT_PROMPT_CACHE_RETENTION: &str = "24h";

pub fn build_responses_request_payload(
    input: Value,
    model: String,
    instructions: Option<String>,
    tools: Option<Vec<Value>>,
    temperature: Option<f64>,
    max_output_tokens: Option<i64>,
    provider: Option<String>,
    thinking_level: Option<String>,
    options: &AiRequestOptions,
) -> Value {
    let reasoning_effort = request_reasoning_effort(
        provider.as_deref(),
        model.as_str(),
        thinking_level.as_deref(),
    );
    let should_request_reasoning_summary =
        should_request_responses_reasoning_summary(provider.as_deref(), model.as_str());
    let input = normalize_responses_request_input(&input);
    let mut payload = json!({
        "model": model,
        "input": input,
        "store": false,
    });
    if let Some(instructions) = normalized_option(instructions.as_deref()) {
        payload["instructions"] = Value::String(instructions);
    }
    let mut has_prompt_cache_key = false;
    if let Some(cache_key) = normalized_option(options.prompt_cache_key.as_deref()) {
        payload["prompt_cache_key"] = Value::String(cache_key);
        has_prompt_cache_key = true;
    }
    if has_prompt_cache_key && options.include_prompt_cache_retention {
        payload["prompt_cache_retention"] = Value::String(CHAT_PROMPT_CACHE_RETENTION.to_string());
    }
    if let Some(tools) = tools.filter(|items| !items.is_empty()) {
        payload["tools"] = Value::Array(tools);
        payload["tool_choice"] = Value::String("auto".to_string());
    }
    if let Some(cwd) = normalized_option(options.request_cwd.as_deref()) {
        payload["cwd"] = Value::String(cwd);
    }
    if let Some(value) = temperature {
        payload["temperature"] = json!(value);
    }
    if let Some(value) = max_output_tokens {
        payload["max_output_tokens"] = json!(value);
    }
    if let Some(level) = reasoning_effort {
        let mut reasoning_payload = json!({ "effort": level });
        if should_request_reasoning_summary {
            reasoning_payload["summary"] = Value::String("auto".to_string());
        }
        payload["reasoning"] = reasoning_payload;
        payload["include"] = json!(["reasoning.encrypted_content"]);
    }
    if let Some(output_format) = options.output_format.clone() {
        payload["text"] = json!({
            "format": responses_json_schema_format(output_format),
        });
    }
    payload["stream"] = Value::Bool(options.stream);
    payload
}

fn should_request_responses_reasoning_summary(provider: Option<&str>, model: &str) -> bool {
    let provider = normalize_provider(provider.unwrap_or("gpt"));
    if provider == "gpt" {
        return true;
    }
    provider == "openai_compatible" && looks_like_openai_reasoning_model(model)
}

fn looks_like_openai_reasoning_model(model: &str) -> bool {
    let normalized = model.trim().to_ascii_lowercase();
    normalized.starts_with("gpt-")
        || normalized.starts_with("o1")
        || normalized.starts_with("o3")
        || normalized.starts_with("o4")
        || normalized.starts_with("o5")
        || normalized.starts_with("codex")
        || normalized.contains("-codex-")
}

fn request_reasoning_effort(
    provider: Option<&str>,
    model: &str,
    thinking_level: Option<&str>,
) -> Option<String> {
    if let Some(capability) = crate::model_reasoning::capabilities_for_model(model) {
        return capability.normalized(thinking_level);
    }
    let provider = normalize_provider(provider.unwrap_or("gpt"));
    let model = model.trim().to_ascii_lowercase();
    if provider == "kimi" {
        if !model.starts_with("kimi-k3") {
            return None;
        }
        let level = normalize_thinking_level("kimi", thinking_level)
            .ok()
            .flatten()?;
        return match level.as_str() {
            // Kimi K3 always reasons; omitting the field is safer than sending
            // an unsupported attempt to disable it.
            "none" => None,
            "low" => Some("low".to_string()),
            "medium" | "high" => Some("high".to_string()),
            "auto" | "xhigh" | "max" => Some("max".to_string()),
            _ => None,
        };
    }
    if provider == "glm" && !model.starts_with("glm-5.2") {
        return None;
    }
    reasoning_effort_for_provider(Some(provider.as_str()), thinking_level)
}

fn responses_json_schema_format(output_format: JsonSchemaOutputFormat) -> Value {
    let mut format = json!({
        "type": "json_schema",
        "name": output_format.name,
        "schema": output_format.schema,
        "strict": output_format.strict,
    });
    if let Some(description) = output_format.description {
        format["description"] = Value::String(description);
    }
    format
}

pub fn normalized_option(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}
