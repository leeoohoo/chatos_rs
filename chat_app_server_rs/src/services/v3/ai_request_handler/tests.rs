use serde_json::json;

use crate::services::ai_common::validate_request_payload_size;

use super::REQUEST_BODY_LIMIT_ENV;

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
