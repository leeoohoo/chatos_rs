use serde_json::{json, Number, Value};

use crate::core::messages::{
    ensure_message_metadata_object, extract_message_tool_calls_for_display,
    is_session_summary_message as is_session_summary, message_has_text_content, message_turn_id,
};
use crate::core::tool_call::extract_tool_call_id;
use crate::models::message::Message;

const TASK_RUNNER_CALLBACK_MESSAGE_MODE: &str = "task_runner_callback";
const TASK_RUNNER_TERMINAL_UPDATE_MESSAGE_KIND: &str = "task_terminal_update";

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

pub(super) fn ensure_message_turn_id(message: &mut Message, turn_id: &str) {
    let normalized_turn_id = turn_id.trim();
    if normalized_turn_id.is_empty() {
        return;
    }

    let metadata = ensure_message_metadata_object(message);
    metadata.insert(
        "conversation_turn_id".to_string(),
        Value::String(normalized_turn_id.to_string()),
    );
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
mod tests {
    use serde_json::json;

    use super::{
        attach_user_history_process_metadata, ensure_message_turn_id,
        strip_assistant_for_compact_history,
    };
    use crate::models::message::Message;

    fn build_message(role: &str, content: &str) -> Message {
        Message::new(
            "session-1".to_string(),
            role.to_string(),
            content.to_string(),
        )
    }

    #[test]
    fn ensure_message_turn_id_overwrites_missing_or_stale_turn_id() {
        let mut message = build_message("assistant", "done");
        message.metadata = Some(json!({
            "conversation_turn_id": "stale-turn"
        }));

        ensure_message_turn_id(&mut message, "turn-42");

        assert_eq!(
            message
                .metadata
                .as_ref()
                .and_then(|value| value.get("conversation_turn_id"))
                .and_then(|value| value.as_str()),
            Some("turn-42")
        );
    }

    #[test]
    fn compact_history_metadata_preserves_turn_stats_and_final_assistant_links() {
        let mut user = build_message("user", "please help");
        user.id = "user-1".to_string();

        let mut assistant = build_message("assistant", "finished");
        assistant.id = "assistant-1".to_string();
        assistant.reasoning = Some("inspect first".to_string());
        assistant.tool_calls = Some(json!([
            {
                "id": "call-1",
                "type": "function",
                "function": {
                    "name": "workspace_search",
                    "arguments": "{\"query\":\"todo\"}"
                }
            }
        ]));

        ensure_message_turn_id(&mut user, "turn-9");
        ensure_message_turn_id(&mut assistant, "turn-9");
        attach_user_history_process_metadata(&mut user, true, 3, 2, 4, Some(assistant.id.clone()));
        strip_assistant_for_compact_history(&mut assistant, &user.id);

        let history_process = user
            .metadata
            .as_ref()
            .and_then(|value| value.get("historyProcess"))
            .expect("historyProcess");
        assert_eq!(
            history_process
                .get("hasProcess")
                .and_then(|value| value.as_bool()),
            Some(true)
        );
        assert_eq!(
            history_process
                .get("toolCallCount")
                .and_then(|value| value.as_u64()),
            Some(3)
        );
        assert_eq!(
            history_process
                .get("thinkingCount")
                .and_then(|value| value.as_u64()),
            Some(2)
        );
        assert_eq!(
            history_process
                .get("processMessageCount")
                .and_then(|value| value.as_u64()),
            Some(4)
        );
        assert_eq!(
            history_process
                .get("finalAssistantMessageId")
                .and_then(|value| value.as_str()),
            Some("assistant-1")
        );
        assert_eq!(
            history_process
                .get("turnId")
                .and_then(|value| value.as_str()),
            Some("turn-9")
        );

        assert!(assistant.tool_calls.is_none());
        assert!(assistant.reasoning.is_none());
        assert_eq!(
            assistant
                .metadata
                .as_ref()
                .and_then(|value| value.get("historyFinalForUserMessageId"))
                .and_then(|value| value.as_str()),
            Some("user-1")
        );
        assert_eq!(
            assistant
                .metadata
                .as_ref()
                .and_then(|value| value.get("historyFinalForTurnId"))
                .and_then(|value| value.as_str()),
            Some("turn-9")
        );
        assert_eq!(
            assistant
                .metadata
                .as_ref()
                .and_then(|value| value.get("conversation_turn_id"))
                .and_then(|value| value.as_str()),
            Some("turn-9")
        );
        assert_eq!(
            assistant
                .metadata
                .as_ref()
                .and_then(|value| value.get("historyProcessExpanded"))
                .and_then(|value| value.as_bool()),
            Some(false)
        );
        assert_eq!(
            assistant
                .metadata
                .as_ref()
                .and_then(|value| value.get("toolCalls"))
                .and_then(|value| value.as_array())
                .map(|items| items.len()),
            Some(1)
        );
    }
}
