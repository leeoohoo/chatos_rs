use serde_json::json;

use super::{
    apply_task_runner_callback_to_user_message,
    build_task_runner_callback_assistant_message_with_contact,
    build_task_runner_callback_message_id, is_task_runner_terminal_event,
    TaskRunnerCallbackRequest,
};
use crate::models::message::Message;

fn sample_callback_payload() -> TaskRunnerCallbackRequest {
    TaskRunnerCallbackRequest {
        event: "task.completed".to_string(),
        task_id: "task-1".to_string(),
        run_id: Some("run-1".to_string()),
        status: "succeeded".to_string(),
        task_title: "Demo task".to_string(),
        project_id: Some("project-1".to_string()),
        task_status: Some("succeeded".to_string()),
        result_summary: Some("done".to_string()),
        error_message: None,
        report_content: None,
        source_session_id: Some("session-1".to_string()),
        source_turn_id: Some("turn-1".to_string()),
        source_user_message_id: Some("user-1".to_string()),
        parent_task_id: None,
        source_run_id: None,
        schedule_mode: Some("once".to_string()),
        prompt: None,
        callback_at: Some("2026-06-10T10:00:00Z".to_string()),
    }
}

fn build_task_runner_callback_assistant_message(
    session_id: &str,
    payload: &TaskRunnerCallbackRequest,
) -> Message {
    build_task_runner_callback_assistant_message_with_contact(session_id, payload, None)
}

#[test]
fn callback_message_id_is_deterministic_for_same_run() {
    let payload = sample_callback_payload();
    let id = build_task_runner_callback_message_id(&payload);
    assert_eq!(
        id,
        "task_runner_callback::user-1::task-1::task.completed::run-1"
    );
}

#[test]
fn callback_assistant_message_carries_idempotent_identity_and_async_metadata() {
    let payload = sample_callback_payload();
    let message = build_task_runner_callback_assistant_message("session-1", &payload);

    assert_eq!(
        message.id,
        "task_runner_callback::user-1::task-1::task.completed::run-1"
    );
    assert_eq!(message.created_at, "2026-06-10T10:00:00Z");
    assert_eq!(
        message
            .metadata
            .as_ref()
            .and_then(|value| value.get("task_runner_async"))
            .and_then(|value| value.get("mode"))
            .and_then(|value| value.as_str()),
        Some("contact_async")
    );
    assert_eq!(
        message
            .metadata
            .as_ref()
            .and_then(|value| value.get("conversation_turn_id"))
            .and_then(|value| value.as_str()),
        Some("turn-1")
    );
    assert_eq!(
        message
            .metadata
            .as_ref()
            .and_then(|value| value.get("task_runner_async"))
            .and_then(|value| value.get("source_turn_id"))
            .and_then(|value| value.as_str()),
        Some("turn-1")
    );
    assert_eq!(
        message
            .metadata
            .as_ref()
            .and_then(|value| value.get("task_runner_async"))
            .and_then(|value| value.get("callback_at"))
            .and_then(|value| value.as_str()),
        Some("2026-06-10T10:00:00Z")
    );
}

#[test]
fn callback_updates_task_tracking_without_overwriting_existing_message_status() {
    let mut message = Message::new(
        "session-1".to_string(),
        "user".to_string(),
        "please handle this".to_string(),
    );
    message.id = "user-1".to_string();
    message.metadata = Some(json!({
        "task_runner_async": {
            "overall_status": "completed"
        }
    }));

    let mut payload = sample_callback_payload();
    payload.event = "task.created".to_string();
    payload.task_id = "task-1".to_string();
    apply_task_runner_callback_to_user_message(&mut message, &payload);

    payload.event = "task.completed".to_string();
    apply_task_runner_callback_to_user_message(&mut message, &payload);

    let task_runner_async = message
        .metadata
        .as_ref()
        .and_then(|value| value.get("task_runner_async"))
        .cloned()
        .unwrap_or_else(|| json!({}));

    assert_eq!(
        task_runner_async
            .get("overall_status")
            .and_then(|value| value.as_str()),
        Some("completed")
    );
    assert_eq!(
        task_runner_async
            .get("created_task_ids")
            .and_then(|value| value.as_array())
            .map(|value| value.len()),
        Some(1)
    );
    assert_eq!(
        task_runner_async
            .get("succeeded_task_ids")
            .and_then(|value| value.as_array())
            .map(|value| value.len()),
        Some(1)
    );
}

#[test]
fn terminal_callback_marks_source_user_message_completed() {
    let mut message = Message::new(
        "session-1".to_string(),
        "user".to_string(),
        "please handle this".to_string(),
    );
    message.id = "user-1".to_string();
    message.metadata = Some(json!({
        "task_runner_async": {
            "overall_status": "processing"
        }
    }));

    let payload = sample_callback_payload();
    apply_task_runner_callback_to_user_message(&mut message, &payload);

    assert_eq!(
        message
            .metadata
            .as_ref()
            .and_then(|value| value.get("task_runner_async"))
            .and_then(|value| value.get("overall_status"))
            .and_then(|value| value.as_str()),
        Some("completed")
    );
}

#[test]
fn task_runner_terminal_event_includes_failed_blocked_and_cancelled() {
    assert!(is_task_runner_terminal_event("task.completed"));
    assert!(is_task_runner_terminal_event("task.failed"));
    assert!(is_task_runner_terminal_event("task.blocked"));
    assert!(is_task_runner_terminal_event("task.cancelled"));
    assert!(!is_task_runner_terminal_event("task.created"));
}

#[test]
fn failed_callback_assistant_message_keeps_error_detail() {
    let mut payload = sample_callback_payload();
    payload.event = "task.failed".to_string();
    payload.status = "failed".to_string();
    payload.result_summary = None;
    payload.error_message = Some("memory batch sync failed".to_string());

    let message = build_task_runner_callback_assistant_message("session-1", &payload);

    assert!(message.content.contains("任务「Demo task」执行失败"));
    assert!(message.content.contains("memory batch sync failed"));
    assert_eq!(
        message
            .metadata
            .as_ref()
            .and_then(|value| value.get("task_runner_async"))
            .and_then(|value| value.get("event"))
            .and_then(|value| value.as_str()),
        Some("task.failed")
    );
}

#[test]
fn callback_message_content_keeps_full_detail() {
    let mut payload = sample_callback_payload();
    payload.result_summary = None;
    payload.report_content = Some("A".repeat(5_000));
    let message = build_task_runner_callback_assistant_message("session-1", &payload);
    let task_runner_async = message
        .metadata
        .as_ref()
        .and_then(|value| value.get("task_runner_async"))
        .cloned()
        .unwrap_or_else(|| json!({}));

    assert!(message.content.chars().count() > 5_000);
    assert_eq!(
        task_runner_async
            .get("detail_source")
            .and_then(|value| value.as_str()),
        Some("report_content")
    );
    assert_eq!(
        task_runner_async
            .get("detail_preview")
            .and_then(|value| value.as_str())
            .map(|value| value.chars().count()),
        Some(5_000)
    );
}
