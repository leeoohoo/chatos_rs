use serde_json::Value;

pub fn payload_has_prompt_cache_retention(payload: &Value) -> bool {
    payload.get("prompt_cache_retention").is_some()
}

pub fn should_retry_without_prompt_cache_retention<T>(
    first_attempt: &Result<T, String>,
    payload: &Value,
) -> bool {
    if !payload_has_prompt_cache_retention(payload) {
        return false;
    }
    match first_attempt {
        Ok(_) => false,
        Err(err) => is_prompt_cache_retention_unsupported_error(err.as_str()),
    }
}

pub fn is_prompt_cache_retention_unsupported_error(err: &str) -> bool {
    let normalized = err.to_ascii_lowercase();
    normalized.contains("prompt_cache_retention")
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
    fn detects_prompt_cache_retention_in_payload() {
        assert!(super::payload_has_prompt_cache_retention(&json!({
            "prompt_cache_retention": "24h"
        })));
        assert!(!super::payload_has_prompt_cache_retention(&json!({
            "stream": true
        })));
    }

    #[test]
    fn retries_only_when_payload_has_retention_and_error_matches() {
        let attempt: Result<(), String> = Err(
            "status 400 Bad Request: Unsupported parameter: prompt_cache_retention".to_string(),
        );
        assert!(super::should_retry_without_prompt_cache_retention(
            &attempt,
            &json!({"prompt_cache_retention": "24h"})
        ));
        assert!(!super::should_retry_without_prompt_cache_retention(
            &attempt,
            &json!({})
        ));
    }

    #[test]
    fn recognizes_unsupported_retention_errors() {
        assert!(super::is_prompt_cache_retention_unsupported_error(
            "status 400: unknown parameter `prompt_cache_retention`",
        ));
        assert!(super::is_prompt_cache_retention_unsupported_error(
            "status 400: prompt_cache_retention is not supported by upstream",
        ));
        assert!(!super::is_prompt_cache_retention_unsupported_error(
            "status 500: upstream timeout",
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
