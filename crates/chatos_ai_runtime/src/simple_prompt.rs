// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde_json::{json, Value};

use crate::{AiRequestHandler, AiResponse, ModelRuntimeConfig, StreamCallbacks};

#[derive(Clone, Default)]
pub struct SimplePromptOptions {
    pub system_prompt: Option<String>,
    pub temperature: Option<f64>,
    pub max_output_tokens: Option<i64>,
    pub max_attempts: Option<usize>,
    pub callbacks: StreamCallbacks,
}

pub async fn run_compatible_prompt_with<F>(
    handler: &AiRequestHandler,
    config: &ModelRuntimeConfig,
    user_prompt: &str,
    options: SimplePromptOptions,
    mut build_input: F,
) -> Result<AiResponse, String>
where
    F: FnMut(&str, bool) -> Value,
{
    let normalized_system_prompt = normalize_optional_text(
        options
            .system_prompt
            .as_deref()
            .or(config.instructions.as_deref()),
    );
    let mut no_system_messages = base_url_disallows_system_messages(config.base_url.as_str());
    let mut input_as_list = base_url_requires_responses_input_list(config.base_url.as_str());
    let max_attempts = options
        .max_attempts
        .unwrap_or(if config.supports_responses { 4 } else { 3 })
        .max(1);
    let mut last_transport_error: Option<String> = None;

    for attempt in 0..max_attempts {
        let wrapped_user_prompt = if no_system_messages {
            wrap_prompt_with_system_context(user_prompt, normalized_system_prompt.as_deref(), true)
        } else {
            user_prompt.to_string()
        };
        let instructions = if no_system_messages {
            None
        } else {
            normalized_system_prompt.clone()
        };
        let input = build_input(wrapped_user_prompt.as_str(), input_as_list);

        match handler
            .handle_request(
                config.base_url.as_str(),
                config.api_key.as_str(),
                input,
                config.supports_responses,
                config.model.clone(),
                instructions,
                None,
                options.temperature.or(config.temperature),
                options.max_output_tokens.or(config.max_output_tokens),
                options.callbacks.clone(),
                Some(config.provider.clone()),
                config.thinking_level.clone(),
                None,
            )
            .await
        {
            Ok(response) => return Ok(response),
            Err(err) => {
                if !input_as_list && is_input_must_be_list_error(err.as_str()) {
                    input_as_list = true;
                    continue;
                }
                if !no_system_messages && is_system_messages_not_allowed_error(err.as_str()) {
                    no_system_messages = true;
                    continue;
                }
                if attempt + 1 < max_attempts && should_retry_transport_error(err.as_str()) {
                    last_transport_error = Some(err);
                    continue;
                }
                return Err(err);
            }
        }
    }

    Err(last_transport_error.unwrap_or_else(|| "AI 请求失败：兼容重试后仍失败".to_string()))
}

pub fn wrap_prompt_with_system_context(
    user_prompt: &str,
    system_prompt: Option<&str>,
    inline_system_context: bool,
) -> String {
    if !inline_system_context {
        return user_prompt.to_string();
    }

    let Some(system_prompt) = normalize_optional_text(system_prompt) else {
        return user_prompt.to_string();
    };

    format!("【系统上下文】\n{}\n\n{}", system_prompt, user_prompt)
}

pub fn build_responses_text_input(user_prompt: &str, input_as_list: bool) -> Value {
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

pub fn base_url_disallows_system_messages(base_url: &str) -> bool {
    let url = base_url.trim().to_lowercase();
    if url.contains("relay.nf.video") || url.contains("nf.video") {
        return true;
    }

    env_flag_enabled("DISABLE_SYSTEM_MESSAGES_FOR_PROXY")
}

pub fn base_url_requires_responses_input_list(base_url: &str) -> bool {
    let url = base_url.trim().to_lowercase();
    if url.contains("relay.nf.video") || url.contains("nf.video") {
        return true;
    }

    env_flag_enabled("FORCE_RESPONSES_INPUT_LIST")
}

pub fn is_system_messages_not_allowed_error(err: &str) -> bool {
    err.to_lowercase()
        .contains("system messages are not allowed")
}

pub fn is_input_must_be_list_error(err: &str) -> bool {
    err.to_lowercase().contains("input must be a list")
}

pub fn should_retry_transport_error(err: &str) -> bool {
    let normalized = err.to_lowercase();
    normalized.contains("error sending request for url")
        || normalized.contains("error decoding response body")
        || normalized.contains("connection closed before message completed")
        || normalized.contains("unexpected eof")
        || normalized.contains("timed out")
        || normalized.contains("status 522")
        || normalized.contains("status 523")
        || normalized.contains("status 524")
        || normalized.contains("error code: 522")
        || normalized.contains("error code: 523")
        || normalized.contains("error code: 524")
}

pub fn select_preferred_response_text<'a>(
    content: &'a str,
    reasoning: Option<&'a str>,
) -> Option<&'a str> {
    if text_has_content(content) {
        return Some(content);
    }

    reasoning.filter(|value| text_has_content(value))
}

fn normalize_optional_text(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(ToOwned::to_owned)
}

fn text_has_content(value: &str) -> bool {
    !value.trim().is_empty()
}

fn env_flag_enabled(key: &str) -> bool {
    std::env::var(key)
        .ok()
        .map(|value| {
            matches!(
                value.trim().to_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        })
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use serde_json::{json, Value};

    use super::{
        base_url_disallows_system_messages, build_responses_text_input,
        is_input_must_be_list_error, is_system_messages_not_allowed_error,
        select_preferred_response_text, should_retry_transport_error,
        wrap_prompt_with_system_context,
    };

    #[test]
    fn detects_relay_domain_prompt_compat_rules() {
        assert!(base_url_disallows_system_messages(
            "https://relay.nf.video/v1"
        ));
        assert!(!base_url_disallows_system_messages(
            "https://api.openai.com/v1"
        ));
    }

    #[test]
    fn builds_text_input_for_responses_transport() {
        assert_eq!(
            build_responses_text_input("hello", false),
            Value::String("hello".to_string())
        );
        assert_eq!(
            build_responses_text_input("hello", true),
            json!([
                {
                    "type": "message",
                    "role": "user",
                    "content": [
                        {
                            "type": "input_text",
                            "text": "hello"
                        }
                    ]
                }
            ])
        );
    }

    #[test]
    fn wraps_user_prompt_with_system_context_when_needed() {
        assert_eq!(
            wrap_prompt_with_system_context("User", Some("System"), true),
            "【系统上下文】\nSystem\n\nUser"
        );
        assert_eq!(
            wrap_prompt_with_system_context("User", Some("System"), false),
            "User"
        );
    }

    #[test]
    fn detects_prompt_compat_error_shapes() {
        assert!(is_input_must_be_list_error(
            "Bad Request: input must be a list"
        ));
        assert!(is_system_messages_not_allowed_error(
            "{\"detail\":\"System messages are not allowed\"}"
        ));
        assert!(should_retry_transport_error("request timed out"));
        assert!(!should_retry_transport_error("status 401: invalid api key"));
    }

    #[test]
    fn selects_visible_content_before_reasoning() {
        assert_eq!(
            select_preferred_response_text("answer", Some("thinking")),
            Some("answer")
        );
        assert_eq!(
            select_preferred_response_text("   ", Some("thinking")),
            Some("thinking")
        );
        assert_eq!(select_preferred_response_text("   ", Some("   ")), None);
    }
}
