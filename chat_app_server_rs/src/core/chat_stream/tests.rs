use serde_json::json;

use super::events::build_error_event_payload;
use super::text::join_stream_text;

#[test]
fn join_stream_text_prefers_longer_snapshot() {
    assert_eq!(join_stream_text("hello", "hello world"), "hello world");
}

#[test]
fn join_stream_text_merges_suffix_overlap() {
    assert_eq!(
        join_stream_text("这是第一段内容ABCDEF", "内容ABCDEF第二段"),
        "这是第一段内容ABCDEF第二段"
    );
}

#[test]
fn build_error_event_payload_marks_rate_limited_errors() {
    let payload = build_error_event_payload(
        "status 429 Too Many Requests: {\"error\":{\"message\":\"Rate limit exceeded\",\"type\":\"bad_response_status_code\",\"code\":\"bad_response_status_code\"}}",
        Some(&json!({"content":"partial"})),
    );

    assert_eq!(
        payload.get("code").and_then(|value| value.as_str()),
        Some("RATE_LIMITED")
    );
    assert!(payload
        .get("message")
        .and_then(|value| value.as_str())
        .map(|value| value.contains("上游模型接口限流"))
        .unwrap_or(false));
    assert_eq!(
        payload
            .get("data")
            .and_then(|value| value.get("detail"))
            .and_then(|value| value.as_str()),
        Some(
            "status 429 Too Many Requests: {\"error\":{\"message\":\"Rate limit exceeded\",\"type\":\"bad_response_status_code\",\"code\":\"bad_response_status_code\"}}"
        )
    );
}
