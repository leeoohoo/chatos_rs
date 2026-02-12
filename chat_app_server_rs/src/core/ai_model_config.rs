use serde_json::Value;

use crate::core::ai_settings::effective_reasoning_enabled;
use crate::utils::model_config::{normalize_provider, normalize_thinking_level};

#[derive(Debug, Clone)]
pub struct ResolvedChatModelConfig {
    pub model: String,
    pub provider: String,
    pub thinking_level: Option<String>,
    pub temperature: f64,
    pub supports_images: bool,
    pub effective_reasoning: bool,
    pub api_key: String,
    pub base_url: String,
    pub system_prompt: Option<String>,
    pub use_active_system_context: bool,
}

pub fn resolve_chat_model_config(
    model_cfg: &Value,
    default_model: &str,
    default_api_key: &str,
    default_base_url: &str,
    request_reasoning_enabled: Option<bool>,
    respect_model_flags: bool,
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

    let supports_reasoning = model_cfg
        .get("supports_reasoning")
        .and_then(|value| value.as_bool())
        .unwrap_or(false);

    let reasoning_enabled = request_reasoning_enabled.unwrap_or_else(|| {
        model_cfg
            .get("reasoning_enabled")
            .and_then(|value| value.as_bool())
            .unwrap_or(true)
    });

    let effective_reasoning = effective_reasoning_enabled(
        supports_reasoning,
        thinking_level.as_deref(),
        reasoning_enabled,
    );

    let api_key = model_cfg
        .get("api_key")
        .and_then(|value| value.as_str())
        .unwrap_or(default_api_key)
        .to_string();

    let base_url = model_cfg
        .get("base_url")
        .and_then(|value| value.as_str())
        .unwrap_or(default_base_url)
        .to_string();

    let system_prompt = model_cfg
        .get("system_prompt")
        .and_then(|value| value.as_str())
        .map(|value| value.to_string());

    let use_active_system_context = if respect_model_flags {
        model_cfg
            .get("use_active_system_context")
            .and_then(|value| value.as_bool())
            .unwrap_or(true)
    } else {
        true
    };

    ResolvedChatModelConfig {
        model,
        provider,
        thinking_level,
        temperature,
        supports_images,
        effective_reasoning,
        api_key,
        base_url,
        system_prompt,
        use_active_system_context,
    }
}

#[cfg(test)]
mod tests {
    use super::resolve_chat_model_config;
    use serde_json::json;

    #[test]
    fn applies_defaults_when_config_is_missing() {
        let resolved = resolve_chat_model_config(
            &json!({}),
            "gpt-4o-mini",
            "k",
            "https://example.com",
            None,
            true,
        );

        assert_eq!(resolved.model, "gpt-4o-mini");
        assert_eq!(resolved.provider, "gpt");
        assert_eq!(resolved.temperature, 0.7);
        assert!(!resolved.supports_images);
        assert!(!resolved.effective_reasoning);
        assert_eq!(resolved.api_key, "k");
        assert_eq!(resolved.base_url, "https://example.com");
        assert!(resolved.use_active_system_context);
    }

    #[test]
    fn request_reasoning_flag_takes_priority() {
        let resolved = resolve_chat_model_config(
            &json!({"supports_reasoning": true, "reasoning_enabled": false}),
            "gpt-4o-mini",
            "k",
            "https://example.com",
            Some(true),
            true,
        );

        assert!(resolved.effective_reasoning);
    }

    #[test]
    fn ignores_model_flags_when_not_respected() {
        let resolved = resolve_chat_model_config(
            &json!({"use_active_system_context": false}),
            "gpt-4o-mini",
            "k",
            "https://example.com",
            None,
            false,
        );

        assert!(resolved.use_active_system_context);
    }
}
