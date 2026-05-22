use serde_json::json;

use crate::services::ai_common::{
    is_non_terminal_response_status, should_persist_assistant_message,
    validate_request_payload_size,
};

use super::{
    build_request_payload, is_prompt_cache_retention_unsupported_error, read_timeout_env_ms,
    should_retry_without_prompt_cache_retention, AiResponse, REQUEST_BODY_LIMIT_ENV,
};

#[test]
fn payload_precheck_accepts_small_payload() {
    let payload = json!({
        "model": "gpt-4o",
        "input": [{"role": "user", "content": [{"type":"input_text","text":"hello"}]}]
    });
    assert!(validate_request_payload_size(&payload, REQUEST_BODY_LIMIT_ENV).is_ok());
}

#[test]
fn payload_precheck_rejects_oversized_payload() {
    let payload = json!({
        "model": "gpt-4o",
        "input": [{"role": "user", "content": [{"type":"input_text","text":"a".repeat(1_700_000)}]}]
    });
    let err =
        validate_request_payload_size(&payload, REQUEST_BODY_LIMIT_ENV).expect_err("should reject");
    assert!(err.contains("request body too large"));
}

#[test]
fn build_request_payload_includes_request_cwd_when_present() {
    let payload = build_request_payload(
        json!([{"role":"user","content":[{"type":"input_text","text":"hello"}]}]),
        "gpt-5.3-codex".to_string(),
        Some("system".to_string()),
        Some("session-123".to_string()),
        None,
        Some("/tmp/worktree".to_string()),
        Some(0.2),
        Some(256),
        Some("gpt".to_string()),
        Some("medium".to_string()),
        true,
        true,
    );

    assert_eq!(
        payload.get("cwd").and_then(|value| value.as_str()),
        Some("/tmp/worktree")
    );
    assert_eq!(
        payload
            .get("prompt_cache_key")
            .and_then(|value| value.as_str()),
        Some("session-123")
    );
    assert_eq!(
        payload
            .get("prompt_cache_retention")
            .and_then(|value| value.as_str()),
        Some("24h")
    );
    assert_eq!(
        payload.get("stream").and_then(|value| value.as_bool()),
        Some(true)
    );
}

#[test]
fn retries_when_provider_reports_prompt_cache_retention_not_supported() {
    let attempt: Result<AiResponse, String> =
        Err("status 400 Bad Request: Unsupported parameter: prompt_cache_retention".to_string());
    let payload = serde_json::json!({
        "prompt_cache_retention": "24h",
    });

    assert!(should_retry_without_prompt_cache_retention(
        &attempt,
        payload.as_object(),
    ));
}

#[test]
fn retries_when_provider_reports_unknown_parameter_wording() {
    assert!(is_prompt_cache_retention_unsupported_error(
        "status 400: unknown parameter `prompt_cache_retention`",
    ));
    assert!(is_prompt_cache_retention_unsupported_error(
        "status 400: prompt_cache_retention is not supported by upstream",
    ));
    assert!(!is_prompt_cache_retention_unsupported_error(
        "status 500: upstream timeout",
    ));
}

#[test]
fn read_timeout_env_ms_clamps_values() {
    std::env::remove_var("AI_V3_TEST_TIMEOUT");
    assert_eq!(read_timeout_env_ms("AI_V3_TEST_TIMEOUT", 12_345), 12_345);

    std::env::set_var("AI_V3_TEST_TIMEOUT", "10");
    assert_eq!(read_timeout_env_ms("AI_V3_TEST_TIMEOUT", 12_345), 1_000);

    std::env::set_var("AI_V3_TEST_TIMEOUT", "9999999");
    assert_eq!(read_timeout_env_ms("AI_V3_TEST_TIMEOUT", 12_345), 600_000);

    std::env::set_var("AI_V3_TEST_TIMEOUT", "abc");
    assert_eq!(read_timeout_env_ms("AI_V3_TEST_TIMEOUT", 12_345), 12_345);

    std::env::remove_var("AI_V3_TEST_TIMEOUT");
}

#[test]
fn build_request_payload_skips_blank_prompt_cache_key() {
    let payload = build_request_payload(
        json!([{"role":"user","content":[{"type":"input_text","text":"hello"}]}]),
        "gpt-5.3-codex".to_string(),
        Some("system".to_string()),
        Some("   ".to_string()),
        None,
        None,
        None,
        None,
        Some("gpt".to_string()),
        Some("medium".to_string()),
        true,
        true,
    );

    assert!(payload.get("prompt_cache_key").is_none());
    assert!(payload.get("prompt_cache_retention").is_none());
}

#[test]
fn build_request_payload_never_includes_prev_id() {
    let payload = build_request_payload(
        json!([{"role":"user","content":[{"type":"input_text","text":"hello"}]}]),
        "gpt-5.3-codex".to_string(),
        Some("system".to_string()),
        None,
        None,
        None,
        None,
        None,
        Some("gpt".to_string()),
        Some("medium".to_string()),
        true,
        true,
    );

    assert!(payload.get("prev_id").is_none());
}

#[test]
fn marks_non_terminal_statuses() {
    assert!(is_non_terminal_response_status(Some("in_progress")));
    assert!(is_non_terminal_response_status(Some("queued")));
    assert!(!is_non_terminal_response_status(Some("completed")));
    assert!(!is_non_terminal_response_status(None));
}

#[test]
fn skips_persist_for_non_terminal_empty_response() {
    assert!(!should_persist_assistant_message(
        "",
        None,
        None,
        Some("in_progress"),
    ));
    assert!(should_persist_assistant_message(
        "hello",
        None,
        None,
        Some("in_progress"),
    ));
    assert!(should_persist_assistant_message(
        "",
        Some("thought"),
        None,
        Some("in_progress"),
    ));
}
