use serde_json::json;

use crate::services::ai_common::validate_request_payload_size;

use super::{build_request_payload, REQUEST_BODY_LIMIT_ENV};

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
        None,
        None,
        Some("/tmp/worktree".to_string()),
        Some(0.2),
        Some(256),
        Some("gpt".to_string()),
        Some("medium".to_string()),
        true,
    );

    assert_eq!(
        payload.get("cwd").and_then(|value| value.as_str()),
        Some("/tmp/worktree")
    );
    assert_eq!(
        payload.get("stream").and_then(|value| value.as_bool()),
        Some(true)
    );
}
