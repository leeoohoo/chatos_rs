// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde_json::Value;

pub fn payload_has_prompt_cache_options(payload: &Value) -> bool {
    payload.get("prompt_cache_key").is_some() || payload.get("prompt_cache_retention").is_some()
}

pub fn should_retry_without_prompt_cache_options<T>(
    first_attempt: &Result<T, String>,
    payload: &Value,
) -> bool {
    if !payload_has_prompt_cache_options(payload) {
        return false;
    }
    match first_attempt {
        Ok(_) => false,
        Err(err) => is_prompt_cache_option_unsupported_error(err.as_str()),
    }
}

pub fn is_prompt_cache_option_unsupported_error(err: &str) -> bool {
    let normalized = err.to_ascii_lowercase();
    (normalized.contains("prompt_cache_key") || normalized.contains("prompt_cache_retention"))
        && (normalized.contains("unsupported parameter")
            || normalized.contains("unknown parameter")
            || normalized.contains("not supported"))
}

pub fn is_previous_response_id_unsupported_error(err: &str) -> bool {
    let normalized = err.to_ascii_lowercase();
    normalized.contains("previous_response_id")
        && (normalized.contains("unsupported parameter")
            || normalized.contains("unknown parameter")
            || normalized.contains("not supported"))
}

pub fn base_url_supports_prompt_cache_retention(base_url: &str) -> bool {
    let normalized = base_url.trim().to_ascii_lowercase();
    normalized.contains("api.openai.com")
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    #[test]
    fn detects_prompt_cache_options_in_payload() {
        assert!(super::payload_has_prompt_cache_options(&json!({
            "prompt_cache_retention": "24h"
        })));
        assert!(super::payload_has_prompt_cache_options(&json!({
            "prompt_cache_key": "run-1"
        })));
        assert!(!super::payload_has_prompt_cache_options(&json!({
            "stream": true
        })));
    }

    #[test]
    fn retries_only_when_payload_has_cache_options_and_error_matches() {
        let attempt: Result<(), String> = Err(
            "status 400 Bad Request: Unsupported parameter: prompt_cache_retention".to_string(),
        );
        assert!(super::should_retry_without_prompt_cache_options(
            &attempt,
            &json!({"prompt_cache_retention": "24h"})
        ));
        assert!(!super::should_retry_without_prompt_cache_options(
            &attempt,
            &json!({})
        ));

        let key_attempt: Result<(), String> =
            Err("status 400: unknown parameter `prompt_cache_key`".to_string());
        assert!(super::should_retry_without_prompt_cache_options(
            &key_attempt,
            &json!({"prompt_cache_key": "run-1"})
        ));
    }

    #[test]
    fn recognizes_unsupported_cache_option_errors() {
        assert!(super::is_prompt_cache_option_unsupported_error(
            "status 400: unknown parameter `prompt_cache_retention`",
        ));
        assert!(super::is_prompt_cache_option_unsupported_error(
            "status 400: prompt_cache_retention is not supported by upstream",
        ));
        assert!(super::is_prompt_cache_option_unsupported_error(
            "status 400: unsupported parameter prompt_cache_key",
        ));
        assert!(!super::is_prompt_cache_option_unsupported_error(
            "status 500: upstream timeout",
        ));
    }

    #[test]
    fn recognizes_unsupported_previous_response_id_errors() {
        assert!(super::is_previous_response_id_unsupported_error(
            "status 400: unknown parameter `previous_response_id`",
        ));
        assert!(super::is_previous_response_id_unsupported_error(
            "status 400: previous_response_id is not supported by upstream",
        ));
        assert!(!super::is_previous_response_id_unsupported_error(
            "status 404: previous response not found",
        ));
    }

    #[test]
    fn enables_retention_only_for_openai_base_url() {
        assert!(super::base_url_supports_prompt_cache_retention(
            "https://api.openai.com/v1"
        ));
        assert!(!super::base_url_supports_prompt_cache_retention(
            "https://api.deepseek.com"
        ));
    }
}
