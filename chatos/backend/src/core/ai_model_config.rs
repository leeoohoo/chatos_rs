// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde_json::Value;

use crate::utils::model_config::{
    default_base_url_for_provider, normalize_provider, normalize_thinking_level,
};

#[derive(Debug, Clone)]
pub struct ResolvedChatModelConfig {
    pub model_config_id: Option<String>,
    pub model: String,
    pub provider: String,
    pub thinking_level: Option<String>,
    pub temperature: f64,
    pub supports_images: bool,
    pub supports_responses: bool,
    pub api_key: String,
    pub base_url: String,
    pub system_prompt: Option<String>,
    pub model_request_max_retries: usize,
}

pub fn resolve_chat_model_config(
    model_cfg: &Value,
    default_model: &str,
    default_api_key: &str,
    default_base_url: &str,
) -> ResolvedChatModelConfig {
    let model = model_cfg
        .get("model_name")
        .and_then(|value| value.as_str())
        .unwrap_or(default_model)
        .to_string();

    let provider = normalize_provider(
        model_cfg
            .get("provider")
            .and_then(|value| value.as_str())
            .unwrap_or("gpt"),
    );
    let thinking_level = normalize_thinking_level(
        &provider,
        model_cfg
            .get("thinking_level")
            .and_then(|value| value.as_str()),
    )
    .ok()
    .flatten();

    let temperature = model_cfg
        .get("temperature")
        .and_then(|value| value.as_f64())
        .unwrap_or(0.7);

    let supports_images = model_cfg
        .get("supports_images")
        .and_then(|value| value.as_bool())
        .unwrap_or(false);

    let supports_responses = model_cfg
        .get("supports_responses")
        .and_then(|value| value.as_bool())
        .unwrap_or(false);

    let api_key = model_cfg
        .get("api_key")
        .and_then(|value| value.as_str())
        .unwrap_or(default_api_key)
        .to_string();

    let base_url = model_cfg
        .get("base_url")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| default_base_url_for_provider(&provider, default_base_url));

    let system_prompt = model_cfg
        .get("system_prompt")
        .and_then(|value| value.as_str())
        .map(|value| value.to_string());

    let model_request_max_retries = model_cfg
        .get("model_request_max_retries")
        .and_then(Value::as_u64)
        .and_then(|value| usize::try_from(value).ok())
        .unwrap_or(chatos_model_transport::DEFAULT_MODEL_REQUEST_MAX_RETRIES);

    ResolvedChatModelConfig {
        model_config_id: None,
        model,
        provider,
        thinking_level,
        temperature,
        supports_images,
        supports_responses,
        api_key,
        base_url,
        system_prompt,
        model_request_max_retries,
    }
}

#[cfg(test)]
mod tests {
    use super::resolve_chat_model_config;
    use serde_json::json;

    #[test]
    fn applies_defaults_when_config_is_missing() {
        let resolved =
            resolve_chat_model_config(&json!({}), "gpt-4o-mini", "k", "https://example.com");

        assert_eq!(resolved.model, "gpt-4o-mini");
        assert_eq!(resolved.provider, "gpt");
        assert_eq!(resolved.temperature, 0.7);
        assert!(!resolved.supports_images);
        assert!(!resolved.supports_responses);
        assert_eq!(resolved.api_key, "k");
        assert_eq!(resolved.base_url, "https://example.com");
    }

    #[test]
    fn fills_provider_default_base_url_when_profile_base_url_is_blank() {
        let resolved = resolve_chat_model_config(
            &json!({"provider": "deepseek", "base_url": ""}),
            "deepseek-chat",
            "k",
            "https://api.openai.com/v1",
        );

        assert_eq!(resolved.base_url, "https://api.deepseek.com");
    }
}
