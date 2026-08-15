// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde_json::Value;

pub const DEFAULT_ATTACHMENT_TOTAL_MAX_BYTES: i64 = 20 * 1024 * 1024;

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
        effective_reasoning_enabled, DEFAULT_ATTACHMENT_TOTAL_MAX_BYTES,
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
}
