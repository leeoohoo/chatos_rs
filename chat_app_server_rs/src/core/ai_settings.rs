use serde_json::Value;

pub fn chat_max_tokens_from_settings(settings: &Value) -> Option<i64> {
    settings
        .get("CHAT_MAX_TOKENS")
        .and_then(|value| value.as_i64())
        .filter(|value| *value > 0)
}

pub fn effective_reasoning_enabled(
    supports_reasoning: bool,
    thinking_level: Option<&str>,
    reasoning_enabled: bool,
) -> bool {
    let has_thinking_level = thinking_level
        .map(|value| !value.trim().is_empty())
        .unwrap_or(false);

    (supports_reasoning || has_thinking_level) && reasoning_enabled
}

#[cfg(test)]
mod tests {
    use super::{chat_max_tokens_from_settings, effective_reasoning_enabled};
    use serde_json::json;

    #[test]
    fn extracts_positive_chat_max_tokens() {
        assert_eq!(
            chat_max_tokens_from_settings(&json!({"CHAT_MAX_TOKENS": 2048})),
            Some(2048)
        );
        assert_eq!(
            chat_max_tokens_from_settings(&json!({"CHAT_MAX_TOKENS": 0})),
            None
        );
    }

    #[test]
    fn computes_effective_reasoning_flag() {
        assert!(effective_reasoning_enabled(true, None, true));
        assert!(effective_reasoning_enabled(false, Some("medium"), true));
        assert!(!effective_reasoning_enabled(false, None, true));
        assert!(!effective_reasoning_enabled(true, Some("high"), false));
    }
}
