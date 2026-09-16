// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde_json::{json, Value};

pub(crate) fn normalize_base_url(input: &str) -> String {
    input.trim().trim_end_matches('/').to_string()
}

pub(crate) fn effective_request_temperature(
    base_url: &str,
    model: &str,
    configured_temperature: f64,
) -> f64 {
    if provider_requires_disabled_thinking(base_url, model) {
        0.6
    } else if provider_requires_unit_temperature(base_url, model) {
        1.0
    } else {
        configured_temperature.clamp(0.0, 2.0)
    }
}

pub(crate) fn provider_requires_unit_temperature(base_url: &str, model: &str) -> bool {
    let normalized_base_url = base_url.trim().to_lowercase();
    let normalized_model = model.trim().to_lowercase();
    normalized_base_url.contains("moonshot.cn") || normalized_model.starts_with("kimi-")
}

pub(crate) fn provider_requires_disabled_thinking(base_url: &str, model: &str) -> bool {
    let normalized_base_url = base_url.trim().to_lowercase();
    let normalized_model = model.trim().to_lowercase();
    let is_kimi_endpoint = normalized_base_url.contains("moonshot.cn")
        || normalized_base_url.contains("moonshot.ai")
        || normalized_base_url.contains("api.kimi.com");
    is_kimi_endpoint
        && (normalized_model.starts_with("kimi-k2.5") || normalized_model.starts_with("kimi-k2.6"))
}

pub(crate) fn build_responses_endpoint(base_url: &str) -> String {
    let normalized = normalize_base_url(base_url);
    if normalized.ends_with("/responses") {
        normalized
    } else {
        format!("{}/responses", normalized)
    }
}

pub(crate) fn build_responses_input(user_prompt: &str, input_as_list: bool) -> Value {
    if !input_as_list {
        return Value::String(user_prompt.to_string());
    }

    json!([
        {
            "type": "message",
            "role": "user",
            "content": [
                {
                    "type": "input_text",
                    "text": user_prompt
                }
            ]
        }
    ])
}
