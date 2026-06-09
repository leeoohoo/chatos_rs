pub use chatos_ai_runtime::model_config::{
    default_base_url_for_provider, normalize_provider, normalize_thinking_level,
    reasoning_effort_for_provider,
};

pub fn is_gpt_provider(provider: &str) -> bool {
    chatos_ai_runtime::model_config::is_gpt_provider(provider)
}

pub fn thinking_mode_for_provider(
    provider: Option<&str>,
    level: Option<&str>,
) -> Option<&'static str> {
    chatos_ai_runtime::model_config::thinking_mode_for_provider(provider, level)
}

#[cfg(test)]
mod tests {
    use super::{
        default_base_url_for_provider, normalize_provider, normalize_thinking_level,
        reasoning_effort_for_provider, thinking_mode_for_provider,
    };

    #[test]
    fn normalizes_provider_aliases() {
        assert_eq!(normalize_provider("openai"), "gpt");
        assert_eq!(normalize_provider("kimik2"), "kimi");
        assert_eq!(normalize_provider("moonshot"), "kimi");
        assert_eq!(normalize_provider("openai-compatible"), "openai_compatible");
    }

    #[test]
    fn maps_provider_default_base_urls() {
        assert_eq!(
            default_base_url_for_provider("deepseek", "https://api.openai.com/v1"),
            "https://api.deepseek.com"
        );
        assert_eq!(
            default_base_url_for_provider("kimi", "https://api.openai.com/v1"),
            "https://api.moonshot.ai/v1"
        );
        assert_eq!(
            default_base_url_for_provider("gpt", "https://gateway.local/v1"),
            "https://gateway.local/v1"
        );
    }

    #[test]
    fn maps_deepseek_thinking_controls() {
        assert_eq!(
            normalize_thinking_level("deepseek", Some("xhigh")).unwrap(),
            Some("max".to_string())
        );
        assert_eq!(
            reasoning_effort_for_provider(Some("deepseek"), Some("medium")).as_deref(),
            Some("high")
        );
        assert_eq!(
            thinking_mode_for_provider(Some("deepseek"), Some("none")),
            Some("disabled")
        );
        assert_eq!(
            thinking_mode_for_provider(Some("deepseek"), Some("max")),
            Some("enabled")
        );
    }

    #[test]
    fn maps_kimi_thinking_controls() {
        assert_eq!(
            normalize_thinking_level("kimik2", Some("auto")).unwrap(),
            Some("auto".to_string())
        );
        assert_eq!(
            reasoning_effort_for_provider(Some("kimi"), Some("auto")),
            None
        );
        assert_eq!(
            thinking_mode_for_provider(Some("kimi"), Some("none")),
            Some("disabled")
        );
    }
}
