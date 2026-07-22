// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde_json::{json, Value};

#[cfg(test)]
mod tests;

use crate::core::chat_runtime::{metadata_string, ChatRuntimeMetadata};
use crate::core::messages::ensure_message_metadata_object;
use crate::core::task_runner_callback_display::{
    detect_task_runner_callback_language, sanitize_user_visible_callback_detail,
    summarize_task_runner_callback_detail, task_runner_callback_completion_detail,
    task_runner_callback_detail_footer, TaskRunnerCallbackLanguage,
};
use crate::models::message::Message;
use crate::models::session::Session;
use crate::services::realtime::publish_chat_stream_event;

use super::{normalize_callback_value, TaskRunnerCallbackRequest};

pub(super) fn apply_task_runner_callback_to_user_message(
    message: &mut Message,
    payload: &TaskRunnerCallbackRequest,
) -> bool {
    let original_metadata = message.metadata.clone();
    let source_user_message_id = message.id.clone();
    let metadata = ensure_message_metadata_object(message);
    let task_runner_meta = ensure_object_field(metadata, "task_runner_async");

    upsert_string(task_runner_meta, "mode", "contact_async");
    upsert_string(
        task_runner_meta,
        "source_user_message_id",
        source_user_message_id.as_str(),
    );
    if let Some(turn_id) = normalize_callback_value(payload.source_turn_id.as_deref()) {
        upsert_string(task_runner_meta, "source_turn_id", turn_id.as_str());
    }
    upsert_string(task_runner_meta, "last_event", payload.event.as_str());
    upsert_string(task_runner_meta, "last_task_id", payload.task_id.as_str());
    if let Some(run_id) = normalize_callback_value(payload.run_id.as_deref()) {
        upsert_string(task_runner_meta, "last_run_id", run_id.as_str());
    }
    if let Some(callback_at) = normalize_callback_value(payload.callback_at.as_deref()) {
        upsert_string(task_runner_meta, "last_event_at", callback_at.as_str());
    }

    let mut created_task_ids = read_string_set(task_runner_meta.get("created_task_ids"));
    let mut running_task_ids = read_string_set(task_runner_meta.get("running_task_ids"));
    let mut terminal_task_ids = read_string_set(task_runner_meta.get("terminal_task_ids"));
    let mut succeeded_task_ids = read_string_set(task_runner_meta.get("succeeded_task_ids"));
    let mut failed_task_ids = read_string_set(task_runner_meta.get("failed_task_ids"));
    let mut blocked_task_ids = read_string_set(task_runner_meta.get("blocked_task_ids"));
    let mut cancelled_task_ids = read_string_set(task_runner_meta.get("cancelled_task_ids"));
    let reset_task_terminal_state =
        |task_id: &str,
         terminal_task_ids: &mut std::collections::BTreeSet<String>,
         succeeded_task_ids: &mut std::collections::BTreeSet<String>,
         failed_task_ids: &mut std::collections::BTreeSet<String>,
         blocked_task_ids: &mut std::collections::BTreeSet<String>,
         cancelled_task_ids: &mut std::collections::BTreeSet<String>| {
            terminal_task_ids.remove(task_id);
            succeeded_task_ids.remove(task_id);
            failed_task_ids.remove(task_id);
            blocked_task_ids.remove(task_id);
            cancelled_task_ids.remove(task_id);
        };

    match payload.event.as_str() {
        "task.created" => {
            created_task_ids.insert(payload.task_id.clone());
        }
        "task.run.started" => {
            created_task_ids.insert(payload.task_id.clone());
            reset_task_terminal_state(
                payload.task_id.as_str(),
                &mut terminal_task_ids,
                &mut succeeded_task_ids,
                &mut failed_task_ids,
                &mut blocked_task_ids,
                &mut cancelled_task_ids,
            );
            running_task_ids.insert(payload.task_id.clone());
        }
        "task.completed" => {
            created_task_ids.insert(payload.task_id.clone());
            running_task_ids.remove(payload.task_id.as_str());
            reset_task_terminal_state(
                payload.task_id.as_str(),
                &mut terminal_task_ids,
                &mut succeeded_task_ids,
                &mut failed_task_ids,
                &mut blocked_task_ids,
                &mut cancelled_task_ids,
            );
            terminal_task_ids.insert(payload.task_id.clone());
            succeeded_task_ids.insert(payload.task_id.clone());
        }
        "task.failed" => {
            created_task_ids.insert(payload.task_id.clone());
            running_task_ids.remove(payload.task_id.as_str());
            reset_task_terminal_state(
                payload.task_id.as_str(),
                &mut terminal_task_ids,
                &mut succeeded_task_ids,
                &mut failed_task_ids,
                &mut blocked_task_ids,
                &mut cancelled_task_ids,
            );
            terminal_task_ids.insert(payload.task_id.clone());
            failed_task_ids.insert(payload.task_id.clone());
        }
        "task.blocked" => {
            created_task_ids.insert(payload.task_id.clone());
            running_task_ids.remove(payload.task_id.as_str());
            reset_task_terminal_state(
                payload.task_id.as_str(),
                &mut terminal_task_ids,
                &mut succeeded_task_ids,
                &mut failed_task_ids,
                &mut blocked_task_ids,
                &mut cancelled_task_ids,
            );
            terminal_task_ids.insert(payload.task_id.clone());
            blocked_task_ids.insert(payload.task_id.clone());
        }
        "task.cancelled" => {
            created_task_ids.insert(payload.task_id.clone());
            running_task_ids.remove(payload.task_id.as_str());
            reset_task_terminal_state(
                payload.task_id.as_str(),
                &mut terminal_task_ids,
                &mut succeeded_task_ids,
                &mut failed_task_ids,
                &mut blocked_task_ids,
                &mut cancelled_task_ids,
            );
            terminal_task_ids.insert(payload.task_id.clone());
            cancelled_task_ids.insert(payload.task_id.clone());
        }
        _ => {}
    }

    let all_created_tasks_terminal = !created_task_ids.is_empty()
        && created_task_ids
            .iter()
            .all(|task_id| terminal_task_ids.contains(task_id));
    upsert_string(
        task_runner_meta,
        "overall_status",
        if all_created_tasks_terminal {
            "completed"
        } else {
            "processing"
        },
    );

    write_string_set(task_runner_meta, "created_task_ids", &created_task_ids);
    write_string_set(task_runner_meta, "running_task_ids", &running_task_ids);
    write_string_set(task_runner_meta, "terminal_task_ids", &terminal_task_ids);
    write_string_set(task_runner_meta, "succeeded_task_ids", &succeeded_task_ids);
    write_string_set(task_runner_meta, "failed_task_ids", &failed_task_ids);
    write_string_set(task_runner_meta, "blocked_task_ids", &blocked_task_ids);
    write_string_set(task_runner_meta, "cancelled_task_ids", &cancelled_task_ids);

    message.metadata != original_metadata
}

pub(super) fn messages_match_for_callback_upsert(existing: &Message, candidate: &Message) -> bool {
    existing.id == candidate.id
        && existing.session_id == candidate.session_id
        && existing.role == candidate.role
        && existing.content == candidate.content
        && existing.message_mode == candidate.message_mode
        && existing.message_source == candidate.message_source
        && existing.summary == candidate.summary
        && existing.tool_calls == candidate.tool_calls
        && existing.tool_call_id == candidate.tool_call_id
        && existing.reasoning == candidate.reasoning
        && existing.metadata == candidate.metadata
        && existing.summary_status == candidate.summary_status
        && existing.summary_id == candidate.summary_id
        && existing.summarized_at == candidate.summarized_at
        && existing.created_at == candidate.created_at
}

fn ensure_object_field<'a>(
    root: &'a mut serde_json::Map<String, Value>,
    key: &str,
) -> &'a mut serde_json::Map<String, Value> {
    let entry = root
        .entry(key.to_string())
        .or_insert_with(|| Value::Object(serde_json::Map::new()));
    if !entry.is_object() {
        *entry = Value::Object(serde_json::Map::new());
    }
    match entry {
        Value::Object(map) => map,
        _ => unreachable!("entry must be object"),
    }
}

fn read_string_set(value: Option<&Value>) -> std::collections::BTreeSet<String> {
    value
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

fn write_string_set(
    root: &mut serde_json::Map<String, Value>,
    key: &str,
    values: &std::collections::BTreeSet<String>,
) {
    root.insert(
        key.to_string(),
        Value::Array(values.iter().cloned().map(Value::String).collect()),
    );
}

fn upsert_string(root: &mut serde_json::Map<String, Value>, key: &str, value: &str) {
    root.insert(key.to_string(), Value::String(value.to_string()));
}

fn normalized_callback_text(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}

fn task_runner_callback_language(
    payload: &TaskRunnerCallbackRequest,
) -> TaskRunnerCallbackLanguage {
    let task_language_text = format!("{}\n{}", payload.task_title, payload.task_objective);
    detect_task_runner_callback_language(
        task_language_text.as_str(),
        Some(payload.fallback_locale.as_str()),
    )
}

fn preferred_callback_detail(
    payload: &TaskRunnerCallbackRequest,
    language: TaskRunnerCallbackLanguage,
) -> Option<(&'static str, &'static str, &str)> {
    if let Some(value) = normalized_callback_text(payload.result_summary.as_deref()) {
        return Some((
            if language.is_english() {
                "Result summary"
            } else {
                "结果摘要"
            },
            "result_summary",
            value,
        ));
    }
    if let Some(value) = normalized_callback_text(payload.report_content.as_deref()) {
        return Some((
            if language.is_english() {
                "Key output"
            } else {
                "关键输出"
            },
            "report_content",
            value,
        ));
    }
    if let Some(value) = normalized_callback_text(payload.error_message.as_deref()) {
        return Some((
            if language.is_english() {
                "Error"
            } else {
                "错误信息"
            },
            "error_message",
            value,
        ));
    }
    None
}

fn user_visible_callback_detail(
    payload: &TaskRunnerCallbackRequest,
    language: TaskRunnerCallbackLanguage,
) -> Option<(&'static str, &'static str, String)> {
    if payload.event == "task.completed" {
        let candidate = normalized_callback_text(payload.result_summary.as_deref())
            .map(|value| ("result_summary", value))
            .or_else(|| {
                normalized_callback_text(payload.report_content.as_deref())
                    .map(|value| ("report_content", value))
            });
        let (detail_source, summary) = candidate
            .and_then(|(source, value)| {
                summarize_task_runner_callback_detail(value, language)
                    .map(|summary| (source, summary))
            })
            .or_else(|| {
                summarize_task_runner_callback_detail(payload.task_objective.as_str(), language)
                    .map(|summary| ("task_objective", summary))
            })
            .unwrap_or_else(|| {
                (
                    "completion_receipt",
                    task_runner_callback_completion_detail(language).to_string(),
                )
            });
        let detail = format!(
            "{}\n{}",
            summary.trim(),
            task_runner_callback_detail_footer(language)
        );
        return Some((
            if language.is_english() {
                "Result summary"
            } else {
                "结果摘要"
            },
            detail_source,
            detail,
        ));
    }
    if payload.event == "task.cancelled" {
        return None;
    }
    preferred_callback_detail(payload, language).map(|(label, source, detail)| {
        (
            label,
            source,
            sanitize_user_visible_callback_detail(detail, language),
        )
    })
}

pub(super) fn is_task_runner_terminal_event(event: &str) -> bool {
    matches!(
        event,
        "task.completed" | "task.failed" | "task.blocked" | "task.cancelled"
    )
}

#[derive(Debug, Clone, Default)]
pub(super) struct TaskRunnerCallbackContactDisplay {
    contact_id: Option<String>,
    contact_agent_id: Option<String>,
    display_name: Option<String>,
}

pub(super) fn build_task_runner_callback_contact_display(
    session: &Session,
) -> TaskRunnerCallbackContactDisplay {
    let runtime = ChatRuntimeMetadata::from_metadata(session.metadata.as_ref());
    let display_name = metadata_string(
        session.metadata.as_ref(),
        &["contact", "agent_name_snapshot"],
    )
    .or_else(|| metadata_string(session.metadata.as_ref(), &["contact", "name"]))
    .or_else(|| {
        metadata_string(
            session.metadata.as_ref(),
            &["ui_contact", "agent_name_snapshot"],
        )
    })
    .or_else(|| metadata_string(session.metadata.as_ref(), &["ui_contact", "name"]))
    .or_else(|| metadata_string(session.metadata.as_ref(), &["chat_runtime", "contact_name"]))
    .or_else(|| normalize_callback_value(Some(session.title.as_str())));

    TaskRunnerCallbackContactDisplay {
        contact_id: runtime.contact_id,
        contact_agent_id: runtime.contact_agent_id,
        display_name,
    }
}

pub(super) fn build_task_runner_callback_assistant_message_with_contact(
    session_id: &str,
    payload: &TaskRunnerCallbackRequest,
    contact_display: Option<&TaskRunnerCallbackContactDisplay>,
) -> Message {
    let mut message = Message::new(
        session_id.to_string(),
        "assistant".to_string(),
        build_task_runner_callback_message_content(payload),
    );
    message.id = build_task_runner_callback_message_id(payload);
    if let Some(callback_at) = normalize_callback_value(payload.callback_at.as_deref()) {
        message.created_at = callback_at;
    }
    message.message_mode = Some("task_runner_callback".to_string());
    message.message_source = Some("task_runner_service".to_string());
    let source_turn_id = normalize_callback_value(payload.source_turn_id.as_deref());
    let metadata = ensure_message_metadata_object(&mut message);
    if let Some(source_turn_id) = source_turn_id.as_deref() {
        upsert_string(metadata, "conversation_turn_id", source_turn_id);
    }
    let task_runner_meta = ensure_object_field(metadata, "task_runner_async");
    upsert_string(task_runner_meta, "mode", "contact_async");
    upsert_string(task_runner_meta, "message_kind", "task_terminal_update");
    if let Some(contact_display) = contact_display {
        if let Some(contact_id) = contact_display.contact_id.as_deref() {
            upsert_string(task_runner_meta, "contact_id", contact_id);
        }
        if let Some(contact_agent_id) = contact_display.contact_agent_id.as_deref() {
            upsert_string(task_runner_meta, "contact_agent_id", contact_agent_id);
        }
        if let Some(display_name) = contact_display.display_name.as_deref() {
            upsert_string(task_runner_meta, "contact_display_name", display_name);
            upsert_string(task_runner_meta, "agent_name_snapshot", display_name);
        }
    }
    upsert_string(task_runner_meta, "event", payload.event.as_str());
    upsert_string(task_runner_meta, "task_id", payload.task_id.as_str());
    upsert_string(task_runner_meta, "status", payload.status.as_str());
    upsert_string(task_runner_meta, "task_title", payload.task_title.as_str());
    if let Some(source_turn_id) = source_turn_id.as_deref() {
        upsert_string(task_runner_meta, "source_turn_id", source_turn_id);
    }
    if let Some(source_user_message_id) =
        normalize_callback_value(payload.source_user_message_id.as_deref())
    {
        upsert_string(
            task_runner_meta,
            "source_user_message_id",
            source_user_message_id.as_str(),
        );
    }
    if let Some(run_id) = normalize_callback_value(payload.run_id.as_deref()) {
        upsert_string(task_runner_meta, "run_id", run_id.as_str());
    }
    if let Some(parent_task_id) = normalize_callback_value(payload.parent_task_id.as_deref()) {
        upsert_string(task_runner_meta, "parent_task_id", parent_task_id.as_str());
    }
    if let Some(source_run_id) = normalize_callback_value(payload.source_run_id.as_deref()) {
        upsert_string(task_runner_meta, "source_run_id", source_run_id.as_str());
    }
    if let Some(schedule_mode) = normalize_callback_value(payload.schedule_mode.as_deref()) {
        upsert_string(task_runner_meta, "schedule_mode", schedule_mode.as_str());
    }
    if let Some(callback_at) = normalize_callback_value(payload.callback_at.as_deref()) {
        upsert_string(task_runner_meta, "callback_at", callback_at.as_str());
    }
    let callback_language = task_runner_callback_language(payload);
    if let Some((_, detail_source, detail)) =
        user_visible_callback_detail(payload, callback_language)
    {
        match detail_source {
            "error_message" => upsert_string(task_runner_meta, "error_message", detail.as_str()),
            "report_content" => upsert_string(task_runner_meta, "report_excerpt", detail.as_str()),
            _ => upsert_string(task_runner_meta, "result_summary", detail.as_str()),
        }
        upsert_string(task_runner_meta, "detail_source", detail_source);
        upsert_string(task_runner_meta, "detail_preview", detail.as_str());
    }
    message
}

fn build_task_runner_callback_message_id(payload: &TaskRunnerCallbackRequest) -> String {
    let source_user_message_id =
        normalize_callback_value(payload.source_user_message_id.as_deref())
            .unwrap_or_else(|| "unknown_user_message".to_string());
    let task_id = payload.task_id.trim();
    let event = payload.event.trim();
    let run_scope = normalize_callback_value(payload.run_id.as_deref())
        .or_else(|| normalize_callback_value(payload.source_run_id.as_deref()))
        .unwrap_or_else(|| payload.status.trim().to_string());
    format!("task_runner_callback::{source_user_message_id}::{task_id}::{event}::{run_scope}")
}

fn build_task_runner_callback_message_content(payload: &TaskRunnerCallbackRequest) -> String {
    let title = payload.task_title.trim();
    let language = task_runner_callback_language(payload);
    let headline = match (language, payload.event.as_str()) {
        (TaskRunnerCallbackLanguage::EnUs, "task.completed") => {
            format!("Task “{title}” completed")
        }
        (TaskRunnerCallbackLanguage::EnUs, "task.failed") => {
            format!("Task “{title}” failed")
        }
        (TaskRunnerCallbackLanguage::EnUs, "task.blocked") => {
            format!("Task “{title}” is blocked")
        }
        (TaskRunnerCallbackLanguage::EnUs, "task.cancelled") => {
            format!("Task “{title}” was cancelled")
        }
        (TaskRunnerCallbackLanguage::EnUs, _) => {
            format!("Task “{title}” status updated")
        }
        (TaskRunnerCallbackLanguage::ZhCn, "task.completed") => {
            format!("任务「{title}」已完成")
        }
        (TaskRunnerCallbackLanguage::ZhCn, "task.failed") => {
            format!("任务「{title}」执行失败")
        }
        (TaskRunnerCallbackLanguage::ZhCn, "task.blocked") => {
            format!("任务「{title}」当前被阻塞")
        }
        (TaskRunnerCallbackLanguage::ZhCn, "task.cancelled") => {
            format!("任务「{title}」已取消")
        }
        (TaskRunnerCallbackLanguage::ZhCn, _) => format!("任务「{title}」状态更新"),
    };
    match user_visible_callback_detail(payload, language) {
        Some((label, _, detail)) => {
            let separator = if language.is_english() { ":" } else { "：" };
            if detail.is_empty() {
                headline
            } else {
                format!("{headline}\n\n{label}{separator}\n{detail}")
            }
        }
        None => headline,
    }
}

pub(super) fn publish_task_runner_callback_realtime(
    user_id: &str,
    session: &crate::models::session::Session,
    turn_id: Option<&str>,
    user_message_id: &str,
    event: &str,
    user_message: Option<&Message>,
    assistant_message: Option<&Message>,
) {
    publish_chat_stream_event(
        user_id,
        session.id.as_str(),
        turn_id,
        session.project_id.as_deref(),
        Some(user_message_id),
        "chat.task_runner.updated",
        "task_runner_callback",
        json!({
            "type": "task_runner_callback",
            "event": event,
            "result": {
                "persisted_user_message": user_message,
                "persisted_user_message_id": user_message.map(|message| message.id.clone()),
                "persisted_assistant_message": assistant_message,
                "persisted_assistant_message_id": assistant_message.map(|message| message.id.clone()),
            }
        }),
    );
}
