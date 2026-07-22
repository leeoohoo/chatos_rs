// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde_json::{json, Value};

use super::events::{build_error_event_payload, select_persisted_turn_messages_from_desc_page};
use super::text::join_stream_text;
use crate::models::message::Message;

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
    assert!(payload
        .get("data")
        .and_then(|value| value.get("detail"))
        .is_some_and(Value::is_null));
}

#[test]
fn build_error_event_payload_redacts_provider_secrets() {
    let payload = build_error_event_payload(
        "status 500 Internal Server Error: {\"error\":{\"message\":\"provider failed; api_key=test-secret; internal_trace=trace-1\"}}",
        None,
    );
    let serialized = payload.to_string();

    assert_eq!(
        payload.get("code").and_then(Value::as_str),
        Some("MODEL_UPSTREAM_UNAVAILABLE")
    );
    assert_eq!(
        payload.get("message").and_then(Value::as_str),
        Some("模型服务暂时不可用，请稍后重试或切换模型。")
    );
    assert!(!serialized.contains("test-secret"));
    assert!(!serialized.contains("internal_trace"));
}

fn message(id: &str, role: &str, turn_id: &str, content: &str) -> Message {
    let mut message = Message::new(
        "session-1".to_string(),
        role.to_string(),
        content.to_string(),
    );
    message.id = id.to_string();
    message.metadata = Some(json!({ "conversation_turn_id": turn_id }));
    message
}

fn hidden_message(id: &str, role: &str, turn_id: &str) -> Message {
    let mut message = message(id, role, turn_id, "hidden");
    message.metadata = Some(json!({
        "conversation_turn_id": turn_id,
        "hidden": true
    }));
    message
}

#[test]
fn persisted_turn_message_selection_uses_desc_page_and_skips_hidden() {
    let mut user_message = None;
    let mut assistant_message = None;
    let page = vec![
        hidden_message("assistant-hidden", "assistant", "turn-2"),
        message("assistant-new", "assistant", "turn-2", "newest"),
        message("assistant-old", "assistant", "turn-2", "older"),
        message("user-1", "user", "turn-2", "question"),
        message("assistant-other", "assistant", "turn-1", "other"),
    ];

    select_persisted_turn_messages_from_desc_page(
        page,
        "turn-2",
        Some("user-1"),
        &mut user_message,
        &mut assistant_message,
    );

    assert_eq!(
        user_message.as_ref().map(|message| message.id.as_str()),
        Some("user-1")
    );
    assert_eq!(
        assistant_message
            .as_ref()
            .map(|message| message.id.as_str()),
        Some("assistant-new")
    );
}
