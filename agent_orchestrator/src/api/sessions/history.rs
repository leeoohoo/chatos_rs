use serde_json::{json, Number, Value};

use crate::models::message::Message;

pub(super) fn parse_bool_query_flag(value: Option<String>) -> bool {
    value
        .as_deref()
        .map(str::trim)
        .map(|raw| {
            let normalized = raw.to_ascii_lowercase();
            matches!(normalized.as_str(), "1" | "true" | "yes" | "on")
        })
        .unwrap_or(false)
}

fn ensure_metadata_object(message: &mut Message) -> &mut serde_json::Map<String, Value> {
    if !matches!(message.metadata, Some(Value::Object(_))) {
        message.metadata = Some(Value::Object(serde_json::Map::new()));
    }

    match message.metadata {
        Some(Value::Object(ref mut map)) => map,
        _ => unreachable!("metadata should be object"),
    }
}

fn is_session_summary(message: &Message) -> bool {
    match &message.metadata {
        Some(Value::Object(map)) => map
            .get("type")
            .and_then(Value::as_str)
            .map(|value| value == "session_summary")
            .unwrap_or(false),
        _ => false,
    }
}

fn extract_tool_call_id(tool_call: &Value) -> Option<String> {
    let Value::Object(map) = tool_call else {
        return None;
    };

    ["id", "tool_call_id", "toolCallId"].iter().find_map(|key| {
        map.get(*key)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
    })
}

fn parse_tool_calls_value(value: &Value) -> Vec<Value> {
    match value {
        Value::Array(items) => items.clone(),
        Value::Object(_) => vec![value.clone()],
        Value::String(raw) => serde_json::from_str::<Value>(raw)
            .ok()
            .map(|parsed| parse_tool_calls_value(&parsed))
            .unwrap_or_default(),
        _ => Vec::new(),
    }
}

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

fn extract_tool_calls_from_message(message: &Message) -> Vec<Value> {
    if let Some(tool_calls) = &message.tool_calls {
        let parsed = parse_tool_calls_value(tool_calls);
        if !parsed.is_empty() {
            return parsed;
        }
    }

    if let Some(Value::Object(map)) = &message.metadata {
        if let Some(value) = map.get("toolCalls").or_else(|| map.get("tool_calls")) {
            let parsed = parse_tool_calls_value(value);
            if !parsed.is_empty() {
                return parsed;
            }
        }
    }

    Vec::new()
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

fn count_assistant_thinking_steps(message: &Message) -> usize {
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
        if let Some(tool_call_id) = extract_tool_call_id(tool_call) {
            segments.push(json!({
                "type": "tool_call",
                "toolCallId": tool_call_id,
            }));
        }
    });

    if !message.content.trim().is_empty() {
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

fn enrich_assistant_message_for_display(message: &mut Message) {
    if message.role != "assistant" || is_session_summary(message) {
        return;
    }

    let tool_calls = extract_tool_calls_from_message(message);
    let segments = build_assistant_segments(message, &tool_calls);

    if !tool_calls.is_empty() {
        message.tool_calls = Some(Value::Array(tool_calls.clone()));
    }

    let metadata = ensure_metadata_object(message);
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

fn select_final_assistant_index(messages: &[Message], start: usize, end: usize) -> Option<usize> {
    let mut fallback_index: Option<usize> = None;

    for index in (start..end).rev() {
        let message = &messages[index];
        if message.role != "assistant" || is_session_summary(message) {
            continue;
        }

        if fallback_index.is_none() {
            fallback_index = Some(index);
        }

        if !message.content.trim().is_empty() {
            return Some(index);
        }
    }

    fallback_index
}

fn message_turn_id(message: &Message) -> Option<&str> {
    message
        .metadata
        .as_ref()
        .and_then(|meta| {
            meta.get("conversation_turn_id")
                .or_else(|| meta.get("conversationTurnId"))
        })
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

fn attach_user_history_process_metadata(
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

    let metadata = ensure_metadata_object(user_message);
    metadata.insert("historyProcess".to_string(), history_process);
}

fn strip_assistant_for_compact_history(message: &mut Message, user_message_id: &str) {
    if message.role != "assistant" {
        return;
    }

    enrich_assistant_message_for_display(message);
    message.reasoning = None;
    message.tool_calls = None;
    let turn_id = message_turn_id(message).map(|id| id.to_string());

    let metadata = ensure_metadata_object(message);
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

fn mark_process_message_loaded(message: &mut Message, user_message_id: &str) {
    let turn_id = message_turn_id(message).map(|value| value.to_string());
    let metadata = ensure_metadata_object(message);
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

fn build_embedded_process_message(
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

    let metadata = ensure_metadata_object(&mut synthetic);
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

fn build_compact_history_messages(messages: Vec<Message>) -> Vec<Message> {
    if messages.is_empty() {
        return messages;
    }

    let user_indexes: Vec<usize> = messages
        .iter()
        .enumerate()
        .filter_map(|(index, message)| (message.role == "user").then_some(index))
        .collect();

    if user_indexes.is_empty() {
        return messages;
    }

    let mut compact = Vec::new();

    for (position, user_index) in user_indexes.iter().enumerate() {
        let next_user_index = if position + 1 < user_indexes.len() {
            user_indexes[position + 1]
        } else {
            messages.len()
        };

        let mut user_message = messages[*user_index].clone();
        let user_message_id = user_message.id.clone();
        let final_assistant_index =
            select_final_assistant_index(&messages, user_index + 1, next_user_index);

        let mut tool_call_count = 0usize;
        let mut thinking_count = 0usize;
        let mut process_message_count = 0usize;

        for index in (user_index + 1)..next_user_index {
            let message = &messages[index];
            if message.role == "assistant" && !is_session_summary(message) {
                tool_call_count += extract_tool_calls_from_message(message).len();
                thinking_count += count_assistant_thinking_steps(message);
            }

            if Some(index) != final_assistant_index
                && (message.role == "assistant" || message.role == "tool")
                && !(message.role == "assistant" && is_session_summary(message))
            {
                process_message_count += 1;
            }
        }

        let final_assistant_message_id =
            final_assistant_index.map(|index| messages[index].id.clone());
        attach_user_history_process_metadata(
            &mut user_message,
            process_message_count > 0 || tool_call_count > 0 || thinking_count > 0,
            tool_call_count,
            thinking_count,
            process_message_count,
            final_assistant_message_id,
        );
        compact.push(user_message);

        for index in (user_index + 1)..next_user_index {
            let source = &messages[index];
            if Some(index) == final_assistant_index {
                let mut assistant = source.clone();
                strip_assistant_for_compact_history(&mut assistant, &user_message_id);
                compact.push(assistant);
            }
        }
    }

    compact
}

pub(super) fn compact_messages_for_display(
    messages: Vec<Message>,
    limit: Option<i64>,
    offset: i64,
) -> Vec<Message> {
    apply_recent_offset_limit(build_compact_history_messages(messages), limit, offset)
}

fn apply_recent_offset_limit(
    messages: Vec<Message>,
    limit: Option<i64>,
    offset: i64,
) -> Vec<Message> {
    let Some(limit) = limit else {
        return messages;
    };

    if limit <= 0 {
        return Vec::new();
    }

    let total = messages.len();
    let offset = offset.max(0) as usize;
    if offset >= total {
        return Vec::new();
    }

    let end = total - offset;
    let mut start = end.saturating_sub(limit as usize);

    if start > 0 {
        let maybe_user_id = messages[start]
            .metadata
            .as_ref()
            .and_then(|meta| meta.get("historyFinalForUserMessageId"))
            .and_then(Value::as_str);

        if let Some(user_message_id) = maybe_user_id {
            if messages[start - 1].id == user_message_id {
                start -= 1;
            }
        }
    }

    messages[start..end].to_vec()
}

pub(super) fn find_user_index_by_turn_id(messages: &[Message], turn_id: &str) -> Option<usize> {
    let normalized = turn_id.trim();
    if normalized.is_empty() {
        return None;
    }

    messages
        .iter()
        .position(|message| message.role == "user" && message_turn_id(message) == Some(normalized))
}

pub(super) fn build_turn_process_messages(messages: &[Message], user_index: usize) -> Vec<Message> {
    let user_message_id = messages[user_index].id.clone();
    let next_user_index = messages
        .iter()
        .enumerate()
        .skip(user_index + 1)
        .find_map(|(index, message)| (message.role == "user").then_some(index))
        .unwrap_or(messages.len());

    let final_assistant_index =
        select_final_assistant_index(messages, user_index + 1, next_user_index);

    let mut process_messages: Vec<Message> = Vec::new();
    for index in (user_index + 1)..next_user_index {
        if Some(index) == final_assistant_index {
            continue;
        }

        let source = &messages[index];
        if source.role == "assistant" && !is_session_summary(source) {
            let mut assistant = source.clone();
            enrich_assistant_message_for_display(&mut assistant);
            mark_process_message_loaded(&mut assistant, &user_message_id);
            process_messages.push(assistant);
        } else if source.role == "tool" {
            let mut tool_message = source.clone();
            mark_process_message_loaded(&mut tool_message, &user_message_id);
            process_messages.push(tool_message);
        }
    }

    if process_messages.is_empty() {
        if let Some(final_assistant_index) = final_assistant_index {
            if let Some(synthetic) =
                build_embedded_process_message(&messages[final_assistant_index], &user_message_id)
            {
                process_messages.push(synthetic);
            }
        }
    }

    process_messages
}
