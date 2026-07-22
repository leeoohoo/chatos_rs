// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde_json::{json, Number, Value};

use crate::core::messages::{
    ensure_message_metadata_object, extract_message_tool_calls_for_display,
    is_session_summary_message as is_session_summary, message_has_text_content, message_turn_id,
};
use crate::core::task_runner_callback_display::{
    detect_task_runner_callback_language, sanitize_user_visible_callback_detail,
    summarize_task_runner_callback_detail, task_runner_callback_completion_detail,
    task_runner_callback_detail_footer,
};
use crate::core::tool_call::extract_tool_call_id;
use crate::models::message::Message;
use crate::services::ai_common::TASK_RUNNER_ASYNC_PLAN_MESSAGE_MODE;

const TASK_RUNNER_CALLBACK_MESSAGE_MODE: &str = "task_runner_callback";
const TASK_RUNNER_TERMINAL_UPDATE_MESSAGE_KIND: &str = "task_terminal_update";
const TASK_RUNNER_ASYNC_PLAN_SUMMARY_MESSAGE_KIND: &str = "plan_summary";

fn parse_content_segments_value(value: &Value) -> Vec<Value> {
    match value {
        Value::Array(items) => items.clone(),
        Value::Object(_) => vec![value.clone()],
        Value::String(raw) => serde_json::from_str::<Value>(raw)
            .ok()
            .map(|parsed| parse_content_segments_value(&parsed))
            .unwrap_or_default(),
        _ => Vec::new(),
    }
}

pub(super) fn extract_tool_calls_from_message(message: &Message) -> Vec<Value> {
    extract_message_tool_calls_for_display(message)
}

pub(super) fn is_task_runner_callback_message(message: &Message) -> bool {
    if message
        .message_mode
        .as_deref()
        .map(str::trim)
        .is_some_and(|value| value == TASK_RUNNER_CALLBACK_MESSAGE_MODE)
    {
        return true;
    }

    message
        .metadata
        .as_ref()
        .and_then(|value| value.get("task_runner_async"))
        .and_then(|value| value.get("message_kind"))
        .and_then(Value::as_str)
        .map(str::trim)
        .is_some_and(|value| value == TASK_RUNNER_TERMINAL_UPDATE_MESSAGE_KIND)
}

pub(super) fn is_task_runner_async_plan_summary_message(message: &Message) -> bool {
    let is_task_runner_async_plan_mode = message
        .message_mode
        .as_deref()
        .map(str::trim)
        .is_some_and(|value| value == TASK_RUNNER_ASYNC_PLAN_MESSAGE_MODE);

    let message_kind = message
        .metadata
        .as_ref()
        .and_then(|value| value.get("task_runner_async"))
        .and_then(|value| value.get("message_kind"))
        .and_then(Value::as_str)
        .map(str::trim);
    if message_kind.is_some_and(|value| value == TASK_RUNNER_ASYNC_PLAN_SUMMARY_MESSAGE_KIND) {
        return true;
    }
    if message_kind.is_some() {
        return false;
    }

    is_task_runner_async_plan_mode && message_has_text_content(message)
}

pub(super) fn normalize_task_runner_async_user_status_for_display(
    message: &mut Message,
    completed_by_turn_messages: bool,
) {
    if message.role != "user" {
        return;
    }

    let Some(Value::Object(metadata)) = message.metadata.as_mut() else {
        return;
    };
    let Some(Value::Object(task_runner_async)) = metadata.get_mut("task_runner_async") else {
        return;
    };
    let mode = task_runner_async
        .get("mode")
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or_default();
    if mode != "contact_async" {
        return;
    }
    let current_status = task_runner_async
        .get("overall_status")
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or_default();
    if current_status == "completed" {
        return;
    }
    let has_terminal_tracking = task_runner_async_has_terminal_tracking(task_runner_async);
    let last_event_is_terminal = task_runner_async
        .get("last_event")
        .and_then(Value::as_str)
        .map(str::trim)
        .is_some_and(|value| {
            matches!(
                value,
                "task.completed" | "task.failed" | "task.blocked" | "task.cancelled"
            )
        });

    if completed_by_turn_messages || has_terminal_tracking || last_event_is_terminal {
        task_runner_async.insert(
            "overall_status".to_string(),
            Value::String("completed".to_string()),
        );
    }
}

pub(crate) fn contact_async_user_status_needs_runtime_reconciliation(message: &Message) -> bool {
    if message.role != "user" {
        return false;
    }
    let Some(Value::Object(task_runner_async)) = message
        .metadata
        .as_ref()
        .and_then(|value| value.get("task_runner_async"))
    else {
        return false;
    };
    if task_runner_async.get("mode").and_then(Value::as_str) != Some("contact_async") {
        return false;
    }
    let current_status = task_runner_async
        .get("overall_status")
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or_default();
    if !matches!(
        current_status,
        "pending" | "processing" | "running" | "in_progress"
    ) {
        return false;
    }

    ![
        "created_task_ids",
        "running_task_ids",
        "queued_task_ids",
        "pending_task_ids",
    ]
    .iter()
    .any(|key| {
        task_runner_async
            .get(*key)
            .and_then(Value::as_array)
            .is_some_and(|items| !items.is_empty())
    })
}

pub(crate) fn reconcile_contact_async_user_status_for_display(
    message: &mut Message,
    snapshot_status: Option<&str>,
    active_in_runtime: bool,
) {
    if !contact_async_user_status_needs_runtime_reconciliation(message) {
        return;
    }
    let next_status = if active_in_runtime {
        Some("processing")
    } else {
        match snapshot_status
            .map(str::trim)
            .map(|value| value.to_ascii_lowercase())
            .as_deref()
        {
            Some("completed" | "succeeded") => Some("completed"),
            Some("failed" | "blocked") => Some("failed"),
            Some("cancelled" | "canceled" | "running") => Some("cancelled"),
            _ => None,
        }
    };
    let Some(next_status) = next_status else {
        return;
    };
    let Some(Value::Object(task_runner_async)) = message
        .metadata
        .as_mut()
        .and_then(|value| value.get_mut("task_runner_async"))
    else {
        return;
    };
    task_runner_async.insert(
        "overall_status".to_string(),
        Value::String(next_status.to_string()),
    );
}

pub(super) fn task_runner_async_user_has_terminal_tracking(message: &Message) -> bool {
    if message.role != "user" {
        return false;
    }
    let Some(Value::Object(task_runner_async)) = message
        .metadata
        .as_ref()
        .and_then(|value| value.get("task_runner_async"))
    else {
        return false;
    };

    task_runner_async_has_terminal_tracking(task_runner_async)
}

fn task_runner_async_has_terminal_tracking(
    task_runner_async: &serde_json::Map<String, Value>,
) -> bool {
    [
        "terminal_task_ids",
        "succeeded_task_ids",
        "failed_task_ids",
        "blocked_task_ids",
        "cancelled_task_ids",
    ]
    .iter()
    .any(|key| {
        task_runner_async
            .get(*key)
            .and_then(Value::as_array)
            .is_some_and(|items| !items.is_empty())
    })
}

fn extract_content_segments_from_message(message: &Message) -> Vec<Value> {
    if let Some(Value::Object(map)) = &message.metadata {
        if let Some(value) = map
            .get("contentSegments")
            .or_else(|| map.get("content_segments"))
        {
            return parse_content_segments_value(value);
        }
    }

    Vec::new()
}

fn is_meaningful_reasoning(reasoning: Option<&str>) -> bool {
    let Some(reasoning) = reasoning.map(str::trim).filter(|value| !value.is_empty()) else {
        return false;
    };

    let normalized = reasoning.to_ascii_lowercase();
    !matches!(
        normalized.as_str(),
        "minimal" | "low" | "medium" | "high" | "detailed"
    )
}

pub(super) fn count_assistant_thinking_steps(message: &Message) -> usize {
    let segments = extract_content_segments_from_message(message);
    let segment_count = segments
        .iter()
        .filter(|segment| {
            let Value::Object(map) = segment else {
                return false;
            };
            if map.get("type").and_then(Value::as_str) != Some("thinking") {
                return false;
            }
            let content = map.get("content").and_then(Value::as_str);
            is_meaningful_reasoning(content)
        })
        .count();

    if segment_count > 0 {
        return segment_count;
    }

    if is_meaningful_reasoning(message.reasoning.as_deref()) {
        1
    } else {
        0
    }
}

fn build_assistant_segments(message: &Message, tool_calls: &[Value]) -> Vec<Value> {
    let mut segments = Vec::new();

    if is_meaningful_reasoning(message.reasoning.as_deref()) {
        let content = message.reasoning.clone().unwrap_or_default();
        segments.push(json!({
            "type": "thinking",
            "content": content,
        }));
    }

    tool_calls.iter().for_each(|tool_call| {
        if let Some(tool_call_id) = extract_tool_call_id(tool_call).map(str::to_string) {
            segments.push(json!({
                "type": "tool_call",
                "toolCallId": tool_call_id,
            }));
        }
    });

    if message_has_text_content(message) {
        segments.push(json!({
            "type": "text",
            "content": message.content,
        }));
    }

    segments
}

fn extract_process_segments_from_message(message: &Message) -> Vec<Value> {
    let existing_segments = extract_content_segments_from_message(message);
    let filtered_existing: Vec<Value> = existing_segments
        .into_iter()
        .filter(|segment| {
            let Value::Object(map) = segment else {
                return false;
            };

            match map.get("type").and_then(Value::as_str) {
                Some("thinking") => {
                    is_meaningful_reasoning(map.get("content").and_then(Value::as_str))
                }
                Some("tool_call") => map
                    .get("toolCallId")
                    .or_else(|| map.get("tool_call_id"))
                    .or_else(|| map.get("toolCallID"))
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .is_some(),
                _ => false,
            }
        })
        .collect();
    if !filtered_existing.is_empty() {
        return filtered_existing;
    }

    let tool_calls = extract_tool_calls_from_message(message);
    build_assistant_segments(message, &tool_calls)
        .into_iter()
        .filter(|segment| {
            let Value::Object(map) = segment else {
                return false;
            };
            matches!(
                map.get("type").and_then(Value::as_str),
                Some("thinking") | Some("tool_call")
            )
        })
        .collect()
}

pub(super) fn enrich_assistant_message_for_display(message: &mut Message) {
    if message.role != "assistant" || is_session_summary(message) {
        return;
    }

    let tool_calls = extract_tool_calls_from_message(message);
    let segments = build_assistant_segments(message, &tool_calls);

    if !tool_calls.is_empty() {
        message.tool_calls = Some(Value::Array(tool_calls.clone()));
    }

    let metadata = ensure_message_metadata_object(message);
    if !tool_calls.is_empty() {
        metadata.insert("toolCalls".to_string(), Value::Array(tool_calls));
    }

    if !segments.is_empty() {
        metadata.insert(
            "contentSegments".to_string(),
            Value::Array(segments.clone()),
        );
        metadata.insert(
            "currentSegmentIndex".to_string(),
            Value::Number(Number::from((segments.len() - 1) as u64)),
        );
    }
}

pub(super) fn normalize_task_runner_callback_for_display(message: &mut Message) {
    if !is_task_runner_callback_message(message) {
        return;
    }

    let task_title = message
        .metadata
        .as_ref()
        .and_then(|value| value.get("task_runner_async"))
        .and_then(|value| value.get("task_title"))
        .and_then(Value::as_str)
        .unwrap_or_default();
    let language_text = format!("{task_title}\n{}", message.content);
    let language = detect_task_runner_callback_language(language_text.as_str(), None);
    let event = message
        .metadata
        .as_ref()
        .and_then(|value| value.get("task_runner_async"))
        .and_then(|value| value.get("event"))
        .and_then(Value::as_str)
        .or_else(|| {
            message
                .id
                .split("::")
                .find(|part| part.starts_with("task."))
        })
        .unwrap_or_default()
        .to_string();
    let stored_detail_source = message
        .metadata
        .as_ref()
        .and_then(|value| value.get("task_runner_async"))
        .and_then(|value| value.get("detail_source"))
        .and_then(Value::as_str)
        .map(ToOwned::to_owned);
    let stored_detail = message
        .metadata
        .as_ref()
        .and_then(|value| value.get("task_runner_async"))
        .and_then(|value| {
            value
                .get("result_summary")
                .or_else(|| value.get("detail_preview"))
                .or_else(|| value.get("report_excerpt"))
        })
        .and_then(Value::as_str)
        .map(ToOwned::to_owned);
    let sanitized_content =
        sanitize_user_visible_callback_detail(message.content.as_str(), language);
    let completed_detail = if event == "task.completed" {
        let headline = sanitized_content
            .lines()
            .find(|line| !line.trim().is_empty())
            .unwrap_or_default()
            .trim();
        let content_detail = sanitized_content
            .lines()
            .skip_while(|line| line.trim() != headline)
            .skip(1)
            .filter(|line| {
                !matches!(
                    line.trim(),
                    "Result summary:" | "Result summary：" | "结果摘要:" | "结果摘要："
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
        let summary = stored_detail
            .as_deref()
            .and_then(|detail| summarize_task_runner_callback_detail(detail, language))
            .or_else(|| summarize_task_runner_callback_detail(content_detail.as_str(), language))
            .unwrap_or_else(|| task_runner_callback_completion_detail(language).to_string());
        let detail = format!(
            "{}\n{}",
            summary.trim(),
            task_runner_callback_detail_footer(language)
        );
        let label = if language.is_english() {
            "Result summary:"
        } else {
            "结果摘要："
        };
        message.content = format!("{headline}\n\n{label}\n{detail}");
        Some(detail)
    } else {
        message.content = sanitized_content;
        None
    };

    let source_turn_id = message_turn_id(message).map(|value| value.to_string());
    let metadata = ensure_message_metadata_object(message);
    metadata.remove("conversation_turn_id");
    metadata.remove("conversationTurnId");
    metadata.remove("historyFinalForUserMessageId");
    metadata.remove("historyFinalForTurnId");
    metadata.remove("historyProcessUserMessageId");
    metadata.remove("historyProcessTurnId");
    metadata.remove("historyProcessPlaceholder");
    if let Some(source_turn_id) = source_turn_id {
        let task_runner_async = metadata
            .entry("task_runner_async".to_string())
            .or_insert_with(|| Value::Object(serde_json::Map::new()));
        if let Value::Object(task_runner_async_map) = task_runner_async {
            task_runner_async_map
                .entry("source_turn_id".to_string())
                .or_insert_with(|| Value::String(source_turn_id));
        }
    }
    if let Some(Value::Object(task_runner_async)) = metadata.get_mut("task_runner_async") {
        if let Some(detail) = completed_detail {
            task_runner_async.insert("result_summary".to_string(), Value::String(detail.clone()));
            task_runner_async.insert(
                "detail_source".to_string(),
                Value::String(stored_detail_source.unwrap_or_else(|| "result_summary".to_string())),
            );
            task_runner_async.insert("detail_preview".to_string(), Value::String(detail));
            task_runner_async.remove("report_excerpt");
        }
        for key in [
            "result_summary",
            "error_message",
            "report_excerpt",
            "detail_preview",
        ] {
            let Some(Value::String(value)) = task_runner_async.get_mut(key) else {
                continue;
            };
            *value = sanitize_user_visible_callback_detail(value.as_str(), language);
        }
    }
}

pub(super) fn select_final_assistant_index(
    messages: &[Message],
    start: usize,
    end: usize,
) -> Option<usize> {
    let mut fallback_index: Option<usize> = None;

    for index in (start..end).rev() {
        let message = &messages[index];
        if message.role != "assistant"
            || is_session_summary(message)
            || is_task_runner_callback_message(message)
        {
            continue;
        }

        if fallback_index.is_none() {
            fallback_index = Some(index);
        }

        if message_has_text_content(message) {
            return Some(index);
        }
    }

    fallback_index
}

pub(super) fn attach_user_history_process_metadata(
    user_message: &mut Message,
    has_process: bool,
    tool_call_count: usize,
    thinking_count: usize,
    process_message_count: usize,
    final_assistant_message_id: Option<String>,
) {
    let user_message_id = user_message.id.clone();
    let mut history_process = json!({
        "hasProcess": has_process,
        "toolCallCount": tool_call_count,
        "thinkingCount": thinking_count,
        "processMessageCount": process_message_count,
        "userMessageId": user_message_id,
        "finalAssistantMessageId": final_assistant_message_id,
    });
    if let Some(turn_id) = message_turn_id(user_message) {
        if let Some(map) = history_process.as_object_mut() {
            map.insert("turnId".to_string(), Value::String(turn_id.to_string()));
        }
    }

    let metadata = ensure_message_metadata_object(user_message);
    metadata.insert("historyProcess".to_string(), history_process);
}

pub(super) fn strip_assistant_for_compact_history(message: &mut Message, user_message_id: &str) {
    if message.role != "assistant" {
        return;
    }

    enrich_assistant_message_for_display(message);
    message.reasoning = None;
    message.tool_calls = None;
    let turn_id = message_turn_id(message).map(|id| id.to_string());

    let metadata = ensure_message_metadata_object(message);
    metadata.remove("tool_calls");
    metadata.remove("hidden");
    metadata.insert(
        "historyFinalForUserMessageId".to_string(),
        Value::String(user_message_id.to_string()),
    );
    if let Some(turn_id) = turn_id {
        metadata.insert("historyFinalForTurnId".to_string(), Value::String(turn_id));
    }
    metadata.insert("historyProcessExpanded".to_string(), Value::Bool(false));
}

pub(super) fn mark_process_message_loaded(message: &mut Message, user_message_id: &str) {
    let turn_id = message_turn_id(message).map(|value| value.to_string());
    let metadata = ensure_message_metadata_object(message);
    metadata.insert("hidden".to_string(), Value::Bool(false));
    metadata.insert("historyProcessPlaceholder".to_string(), Value::Bool(false));
    metadata.insert(
        "historyProcessUserMessageId".to_string(),
        Value::String(user_message_id.to_string()),
    );
    metadata.insert("historyProcessLoaded".to_string(), Value::Bool(true));
    if let Some(turn_id) = turn_id {
        metadata.insert("historyProcessTurnId".to_string(), Value::String(turn_id));
    }
}

pub(super) fn build_embedded_process_message(
    final_assistant: &Message,
    user_message_id: &str,
) -> Option<Message> {
    if final_assistant.role != "assistant" || is_session_summary(final_assistant) {
        return None;
    }

    let process_segments = extract_process_segments_from_message(final_assistant);
    let tool_calls = extract_tool_calls_from_message(final_assistant);
    if process_segments.is_empty() && tool_calls.is_empty() {
        return None;
    }

    let mut synthetic = final_assistant.clone();
    synthetic.id = format!("{}::embedded_process", final_assistant.id);
    synthetic.content.clear();
    synthetic.summary = None;
    synthetic.reasoning = None;
    synthetic.tool_calls = (!tool_calls.is_empty()).then_some(Value::Array(tool_calls.clone()));

    let metadata = ensure_message_metadata_object(&mut synthetic);
    metadata.remove("historyFinalForUserMessageId");
    metadata.remove("historyFinalForTurnId");
    metadata.remove("historyProcessExpanded");
    if !tool_calls.is_empty() {
        metadata.insert("toolCalls".to_string(), Value::Array(tool_calls));
    }
    metadata.insert(
        "contentSegments".to_string(),
        Value::Array(process_segments.clone()),
    );
    metadata.insert(
        "currentSegmentIndex".to_string(),
        Value::Number(Number::from(process_segments.len().saturating_sub(1) as u64)),
    );

    mark_process_message_loaded(&mut synthetic, user_message_id);
    Some(synthetic)
}

#[cfg(test)]
include!("history_process_support.test.rs");
