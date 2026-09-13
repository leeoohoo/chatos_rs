// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde_json::{json, Number, Value};

use crate::core::messages::{
    ensure_message_metadata_object, extract_message_tool_calls_for_display,
    is_session_summary_message as is_session_summary, message_has_text_content, message_turn_id,
};
use crate::core::tool_call::extract_tool_call_id;
use crate::models::message::Message;

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
    !matches!(
        reasoning.to_ascii_lowercase().as_str(),
        "minimal" | "low" | "medium" | "high" | "detailed"
    )
}

pub(super) fn count_assistant_thinking_steps(message: &Message) -> usize {
    let segment_count = extract_content_segments_from_message(message)
        .iter()
        .filter(|segment| {
            let Value::Object(map) = segment else {
                return false;
            };
            map.get("type").and_then(Value::as_str) == Some("thinking")
                && is_meaningful_reasoning(map.get("content").and_then(Value::as_str))
        })
        .count();
    if segment_count > 0 {
        segment_count
    } else {
        usize::from(is_meaningful_reasoning(message.reasoning.as_deref()))
    }
}

fn build_assistant_segments(message: &Message, tool_calls: &[Value]) -> Vec<Value> {
    let mut segments = Vec::new();
    if is_meaningful_reasoning(message.reasoning.as_deref()) {
        segments.push(json!({
            "type": "thinking",
            "content": message.reasoning.clone().unwrap_or_default(),
        }));
    }
    for tool_call in tool_calls {
        if let Some(tool_call_id) = extract_tool_call_id(tool_call).map(str::to_string) {
            segments.push(json!({
                "type": "tool_call",
                "toolCallId": tool_call_id,
            }));
        }
    }
    if message_has_text_content(message) {
        segments.push(json!({
            "type": "text",
            "content": message.content,
        }));
    }
    segments
}

fn extract_process_segments_from_message(message: &Message) -> Vec<Value> {
    let existing = extract_content_segments_from_message(message)
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
                    .is_some_and(|value| !value.trim().is_empty()),
                _ => false,
            }
        })
        .collect::<Vec<_>>();
    if !existing.is_empty() {
        return existing;
    }

    let tool_calls = extract_tool_calls_from_message(message);
    build_assistant_segments(message, &tool_calls)
        .into_iter()
        .filter(|segment| {
            matches!(
                segment.get("type").and_then(Value::as_str),
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
            Value::Number(Number::from(segments.len().saturating_sub(1) as u64)),
        );
    }
}

pub(super) fn select_final_assistant_index(
    messages: &[Message],
    start: usize,
    end: usize,
) -> Option<usize> {
    let mut fallback = None;
    for index in (start..end).rev() {
        let message = &messages[index];
        if message.role != "assistant" || is_session_summary(message) {
            continue;
        }
        fallback.get_or_insert(index);
        if message_has_text_content(message) {
            return Some(index);
        }
    }
    fallback
}

pub(super) fn attach_user_history_process_metadata(
    user_message: &mut Message,
    has_process: bool,
    tool_call_count: usize,
    thinking_count: usize,
    process_message_count: usize,
    final_assistant_message_id: Option<String>,
) {
    let mut history_process = json!({
        "hasProcess": has_process,
        "toolCallCount": tool_call_count,
        "thinkingCount": thinking_count,
        "processMessageCount": process_message_count,
        "userMessageId": user_message.id,
        "finalAssistantMessageId": final_assistant_message_id,
    });
    if let Some(turn_id) = message_turn_id(user_message) {
        history_process["turnId"] = Value::String(turn_id.to_string());
    }
    ensure_message_metadata_object(user_message)
        .insert("historyProcess".to_string(), history_process);
}

pub(super) fn strip_assistant_for_compact_history(message: &mut Message, user_message_id: &str) {
    if message.role != "assistant" {
        return;
    }
    enrich_assistant_message_for_display(message);
    message.reasoning = None;
    message.tool_calls = None;
    let turn_id = message_turn_id(message).map(str::to_string);
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
    let turn_id = message_turn_id(message).map(str::to_string);
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
