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
    normalized_base_url.contains("moonshot.cn") && normalized_model.starts_with("kimi-")
}

pub(crate) fn provider_supports_optional_thinking(base_url: &str, model: &str) -> bool {
    provider_requires_disabled_thinking(base_url, model)
}

pub(crate) fn build_chat_completions_endpoint(base_url: &str) -> String {
    let normalized = normalize_base_url(base_url);
    if normalized.ends_with("/chat/completions") {
        normalized
    } else {
        format!("{}/chat/completions", normalized)
    }
}

pub(crate) fn build_responses_endpoint(base_url: &str) -> String {
    let normalized = normalize_base_url(base_url);
    if normalized.ends_with("/responses") {
        normalized
    } else {
        format!("{}/responses", normalized)
    }
}

pub(crate) fn build_chat_messages(
    system_prompt: &str,
    user_prompt: &str,
    no_system_messages: bool,
) -> Value {
    if !no_system_messages {
        return json!([
            {"role": "system", "content": system_prompt},
            {"role": "user", "content": user_prompt}
        ]);
    }

    let mut messages = Vec::new();
    let normalized_system = system_prompt.trim();
    if !normalized_system.is_empty() {
        messages.push(json!({
            "role": "user",
            "content": format!("【系统上下文】\n{}", normalized_system)
        }));
    }
    messages.push(json!({"role": "user", "content": user_prompt}));
    Value::Array(messages)
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

pub(crate) fn base_url_disallows_system_messages(base_url: &str) -> bool {
    let url = base_url.trim().to_lowercase();
    url.contains("relay.nf.video") || url.contains("nf.video")
}

pub(crate) fn base_url_requires_responses_input_list(base_url: &str) -> bool {
    let url = base_url.trim().to_lowercase();
    url.contains("relay.nf.video") || url.contains("nf.video")
}
