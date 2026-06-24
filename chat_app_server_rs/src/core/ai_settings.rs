use serde_json::Value;

pub const DEFAULT_ATTACHMENT_TOTAL_MAX_BYTES: i64 = 20 * 1024 * 1024;
const REQUEST_BODY_BASE64_EXPANSION_NUMERATOR: usize = 4;
const REQUEST_BODY_BASE64_EXPANSION_DENOMINATOR: usize = 3;
const REQUEST_BODY_FIXED_OVERHEAD_BYTES: usize = 1024 * 1024;

pub fn chat_max_tokens_from_settings(settings: &Value) -> Option<i64> {
    settings
        .get("CHAT_MAX_TOKENS")
        .and_then(|value| value.as_i64())
        .filter(|value| *value > 0)
}

pub fn attachment_total_max_bytes_from_settings(settings: &Value) -> i64 {
    settings
        .get("ATTACHMENT_TOTAL_MAX_BYTES")
        .and_then(|value| value.as_i64())
        .filter(|value| *value > 0)
        .unwrap_or(DEFAULT_ATTACHMENT_TOTAL_MAX_BYTES)
}

pub fn request_body_limit_bytes_for_attachment_total(attachment_total_max_bytes: i64) -> usize {
    let attachment_bytes = attachment_total_max_bytes.max(1) as usize;
    let base64_budget = attachment_bytes
        .saturating_mul(REQUEST_BODY_BASE64_EXPANSION_NUMERATOR)
        .saturating_add(REQUEST_BODY_BASE64_EXPANSION_DENOMINATOR - 1)
        / REQUEST_BODY_BASE64_EXPANSION_DENOMINATOR;
    base64_budget.saturating_add(REQUEST_BODY_FIXED_OVERHEAD_BYTES)
}

pub fn request_body_limit_bytes_from_settings(settings: &Value) -> usize {
    request_body_limit_bytes_for_attachment_total(attachment_total_max_bytes_from_settings(
        settings,
    ))
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
    use super::{
        attachment_total_max_bytes_from_settings, chat_max_tokens_from_settings,
        effective_reasoning_enabled, request_body_limit_bytes_for_attachment_total,
        DEFAULT_ATTACHMENT_TOTAL_MAX_BYTES,
    };
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

    #[test]
    fn extracts_attachment_total_limit_with_default() {
        assert_eq!(
            attachment_total_max_bytes_from_settings(&json!({})),
            DEFAULT_ATTACHMENT_TOTAL_MAX_BYTES
        );
        assert_eq!(
            attachment_total_max_bytes_from_settings(&json!({"ATTACHMENT_TOTAL_MAX_BYTES": 1024})),
            1024
        );
        assert_eq!(
            attachment_total_max_bytes_from_settings(&json!({"ATTACHMENT_TOTAL_MAX_BYTES": 0})),
            DEFAULT_ATTACHMENT_TOTAL_MAX_BYTES
        );
    }

    #[test]
    fn derives_request_body_limit_from_attachment_total() {
        assert_eq!(
            request_body_limit_bytes_for_attachment_total(3),
            4 + 1024 * 1024
        );
    }
}
