// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

#[test]
fn normalize_turn_id_trims_and_filters_empty_values() {
    assert_eq!(
        normalize_turn_id(Some("  turn-1 ")),
        Some("turn-1".to_string())
    );
    assert_eq!(normalize_turn_id(Some("   ")), None);
    assert_eq!(normalize_turn_id(None), None);
}

#[test]
fn build_assistant_message_metadata_skips_empty_fields() {
    assert!(build_assistant_message_metadata(None, None, None, None, None).is_none());
    assert!(build_assistant_message_metadata(None, Some("   "), None, None, None).is_none());
}

#[test]
fn build_assistant_message_metadata_keeps_response_id_and_tool_calls() {
    let tool_calls = serde_json::json!([{"id": "call_1"}]);
    let metadata = build_assistant_message_metadata(
        Some(&tool_calls),
        Some("resp_123"),
        Some("turn_123"),
        Some("completed"),
        None,
    );

    assert_eq!(
        metadata
            .as_ref()
            .and_then(|value| value.get("response_id"))
            .and_then(|value| value.as_str()),
        Some("resp_123")
    );
    assert_eq!(
        metadata
            .as_ref()
            .and_then(|value| value.get("toolCalls"))
            .cloned(),
        Some(tool_calls)
    );
    assert_eq!(
        metadata
            .as_ref()
            .and_then(|value| value.get("conversation_turn_id"))
            .and_then(|value| value.as_str()),
        Some("turn_123")
    );
    assert_eq!(
        metadata
            .as_ref()
            .and_then(|value| value.get("response_status"))
            .and_then(|value| value.as_str()),
        Some("completed")
    );
}

#[test]
fn normalize_task_runner_async_plan_metadata_marks_plan_summary_mode() {
    let metadata = normalize_task_runner_async_plan_metadata(Some(json!({
        "response_id": "resp_1"
    })))
    .expect("metadata");

    assert_eq!(
        metadata.get("response_id").and_then(|value| value.as_str()),
        Some("resp_1")
    );
    assert_eq!(
        metadata
            .get("task_runner_async")
            .and_then(|value| value.get("mode"))
            .and_then(|value| value.as_str()),
        Some("contact_async")
    );
    assert_eq!(
        metadata
            .get("task_runner_async")
            .and_then(|value| value.get("message_kind"))
            .and_then(|value| value.as_str()),
        Some("plan_summary")
    );
}

#[test]
fn normalize_task_runner_async_tool_call_metadata_marks_tool_call_mode() {
    let metadata = normalize_task_runner_async_tool_call_metadata(Some(json!({
        "response_id": "resp_1"
    })))
    .expect("metadata");

    assert_eq!(
        metadata.get("response_id").and_then(|value| value.as_str()),
        Some("resp_1")
    );
    assert_eq!(
        metadata
            .get("task_runner_async")
            .and_then(|value| value.get("mode"))
            .and_then(|value| value.as_str()),
        Some("contact_async")
    );
    assert_eq!(
        metadata
            .get("task_runner_async")
            .and_then(|value| value.get("message_kind"))
            .and_then(|value| value.as_str()),
        Some("tool_call")
    );
}

#[test]
fn build_ai_client_success_payload_preserves_response_shape() {
    let payload = build_ai_client_success_payload(
        "hello".to_string(),
        Some("think".to_string()),
        Some("stop".to_string()),
        2,
    );

    assert_eq!(
        payload.get("success").and_then(|value| value.as_bool()),
        Some(true)
    );
    assert_eq!(
        payload.get("content").and_then(|value| value.as_str()),
        Some("hello")
    );
    assert_eq!(
        payload.get("reasoning").and_then(|value| value.as_str()),
        Some("think")
    );
    assert_eq!(
        payload
            .get("finish_reason")
            .and_then(|value| value.as_str()),
        Some("stop")
    );
    assert_eq!(
        payload.get("iteration").and_then(|value| value.as_i64()),
        Some(2)
    );
    assert_eq!(payload.get("tool_calls"), Some(&Value::Null));
}

#[test]
fn attach_ai_client_success_extra_merges_fields() {
    let payload = build_ai_client_success_payload(
        "hello".to_string(),
        Some("think".to_string()),
        Some("stop".to_string()),
        2,
    );
    let merged = attach_ai_client_success_extra(
        payload,
        json!({
            "task_turn_review": {
                "attempted": true,
                "outcome": "pass",
                "rounds": 1
            }
        }),
    );
    assert_eq!(
        merged
            .get("task_turn_review")
            .and_then(|value| value.get("outcome"))
            .and_then(Value::as_str),
        Some("pass")
    );
    assert_eq!(merged.get("content").and_then(Value::as_str), Some("hello"));
}
