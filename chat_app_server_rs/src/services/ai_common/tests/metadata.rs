// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

#[test]
fn build_abort_token_only_cancels_matching_turn() {
    let session_id = "build_abort_token_turn_match";
    abort_registry::clear(session_id);

    let token = build_abort_token(Some(session_id), Some("turn_new")).expect("token");
    assert!(!token.is_cancelled());

    assert!(!abort_registry::abort_turn(session_id, Some("turn_old")));
    assert!(!token.is_cancelled());

    assert!(abort_registry::abort_turn(session_id, Some("turn_new")));
    assert!(token.is_cancelled());

    abort_registry::clear(session_id);
}

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
fn truncate_log_adds_suffix_when_exceeding_limit() {
    let value = truncate_log("abcdefgh", 4);
    assert_eq!(value, "abcd...[truncated]");
    assert_eq!(truncate_log("abc", 4), "abc");
}

#[test]
fn format_error_response_formats_status_and_body_preview() {
    let err = format_error_response(
        reqwest::StatusCode::BAD_GATEWAY,
        "upstream provider failure",
    );
    assert_eq!(err, "status 502 Bad Gateway: upstream provider failure");
}

#[test]
fn completion_failed_error_uses_finish_reason_and_preview() {
    let err = completion_failed_error(Some("failed"), "", Some("detailed reasoning"), None)
        .expect("should return error");
    assert!(err.contains("finish_reason=failed"));
    assert!(err.contains("reasoning_preview=detailed reasoning"));

    let err =
        completion_failed_error(Some("error"), "body", None, None).expect("should return error");
    assert!(err.contains("content_preview=body"));

    let provider_error = json!({
        "code": "context_length_exceeded",
        "type": "invalid_request_error",
        "message": "too long",
        "param": "input"
    });
    let err = completion_failed_error(Some("failed"), "", None, Some(&provider_error))
        .expect("should include provider error");
    assert!(err.contains("provider_error=code=context_length_exceeded"));
    assert!(err.contains("type=invalid_request_error"));
    assert!(err.contains("message=too long"));

    assert!(completion_failed_error(Some("stop"), "", None, None).is_none());
}

#[test]
fn terminal_empty_response_error_detects_terminal_empty_payload() {
    let err = terminal_empty_response_error(Some("completed"), "", None, None, None)
        .expect("terminal empty response should fail");
    assert!(err.contains("terminal empty response"));
    assert!(err.contains("finish_reason=completed"));

    assert!(terminal_empty_response_error(Some("in_progress"), "", None, None, None).is_none());
    assert!(terminal_empty_response_error(Some("completed"), "hello", None, None, None).is_none());
    assert!(
        terminal_empty_response_error(Some("completed"), "", Some("thought"), None, None).is_none()
    );
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
fn should_persist_assistant_message_skips_empty_non_terminal_responses() {
    assert!(!should_persist_assistant_message(
        "",
        None,
        None,
        Some("in_progress"),
    ));
    assert!(!should_persist_assistant_message(
        "",
        None,
        None,
        Some("completed"),
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
        Some("queued"),
    ));
}

#[test]
fn task_runner_async_plan_message_mode_matches_expected_value() {
    assert!(is_task_runner_async_plan_message_mode(Some(
        TASK_RUNNER_ASYNC_PLAN_MESSAGE_MODE
    )));
    assert!(is_task_runner_async_plan_message_mode(Some(
        " task_runner_async_plan "
    )));
    assert!(!is_task_runner_async_plan_message_mode(Some("model")));
    assert!(!is_task_runner_async_plan_message_mode(None));
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

#[test]
fn parsed_stream_response_is_empty_only_when_everything_is_blank() {
    assert!(parsed_stream_response_is_empty(0, " ", "\n", false));
    assert!(!parsed_stream_response_is_empty(1, "", "", false));
    assert!(!parsed_stream_response_is_empty(0, "hello", "", false));
    assert!(!parsed_stream_response_is_empty(0, "", "thinking", false));
    assert!(!parsed_stream_response_is_empty(0, "", "", true));
}

#[tokio::test]
async fn persist_user_message_and_build_content_parts_preserves_turn_metadata() {
    let captured = Arc::new(std::sync::Mutex::new(None::<Value>));
    let holder = captured.clone();

    let prepared = persist_user_message_and_build_content_parts(
        "session_1",
        "hello world",
        "gpt-5.3-codex",
        Vec::new(),
        Some(true),
        Some("turn_1".to_string()),
        move |metadata| {
            let holder = holder.clone();
            async move {
                *holder.lock().expect("lock poisoned") = metadata;
                Ok::<(), String>(())
            }
        },
    )
    .await
    .expect("prepared input");

    assert_eq!(prepared.turn_id.as_deref(), Some("turn_1"));
    assert_eq!(
        captured
            .lock()
            .expect("lock poisoned")
            .as_ref()
            .and_then(|value| value.get("conversation_turn_id"))
            .and_then(|value| value.as_str()),
        Some("turn_1")
    );
    assert_eq!(
        prepared
            .content_parts
            .as_array()
            .and_then(|items| items.first())
            .and_then(|item| item.get("text"))
            .and_then(|value| value.as_str()),
        Some("hello world")
    );
}
