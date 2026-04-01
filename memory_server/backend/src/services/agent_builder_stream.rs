use axum::http::StatusCode;
use serde_json::{json, Map, Value};

use super::{bad_gateway_error, support::parse_json_candidate, ToolCall};

pub(super) async fn read_sse_json_events(
    mut response: reqwest::Response,
) -> Result<Vec<Value>, (StatusCode, String)> {
    let mut buffer = String::new();
    let mut events: Vec<Value> = Vec::new();

    while let Some(bytes) = response
        .chunk()
        .await
        .map_err(|err| bad_gateway_error(format!("agent builder ai stream read failed: {err}")))?
    {
        let text = String::from_utf8_lossy(&bytes).to_string();
        buffer.push_str(text.as_str());
        events.extend(drain_sse_json_events(&mut buffer));
    }

    flush_sse_tail_events(&mut buffer, &mut events);

    if events.is_empty() {
        return Err(bad_gateway_error(
            "agent builder ai stream parse failed: no JSON events found",
        ));
    }

    Ok(events)
}

fn drain_sse_json_events(buffer: &mut String) -> Vec<Value> {
    let mut events = Vec::new();

    while let Some(idx) = buffer.find("\n\n") {
        let packet = buffer[..idx].to_string();
        *buffer = buffer[idx + 2..].to_string();

        for line in packet.lines() {
            let normalized = line.trim();
            if !normalized.starts_with("data:") {
                continue;
            }
            let data = normalized.trim_start_matches("data:").trim();
            if data.is_empty() || data == "[DONE]" {
                continue;
            }
            if let Ok(value) = serde_json::from_str::<Value>(data) {
                events.push(value);
            }
        }
    }

    events
}

fn flush_sse_tail_events(buffer: &mut String, events: &mut Vec<Value>) {
    if buffer.trim().is_empty() {
        return;
    }

    if buffer.contains("data:") {
        if !buffer.ends_with("\n\n") {
            buffer.push_str("\n\n");
        }
        events.extend(drain_sse_json_events(buffer));
    }

    let tail = buffer.trim();
    if tail.is_empty() {
        return;
    }

    if let Ok(value) = serde_json::from_str::<Value>(tail) {
        emit_tail_json_value(value, events);
    }
    buffer.clear();
}

fn emit_tail_json_value(value: Value, events: &mut Vec<Value>) {
    if let Some(items) = value.as_array() {
        for item in items {
            if item.is_object() {
                events.push(item.clone());
            }
        }
        return;
    }
    if value.is_object() {
        events.push(value);
    }
}

pub(super) fn aggregate_chat_completions_stream(
    events: &[Value],
) -> Result<Value, (StatusCode, String)> {
    #[derive(Default, Clone)]
    struct ToolCallAccumulator {
        id: Option<String>,
        name: Option<String>,
        arguments: String,
    }

    let mut content = String::new();
    let mut finish_reason: Option<String> = None;
    let mut usage: Option<Value> = None;
    let mut tool_calls: Vec<ToolCallAccumulator> = Vec::new();

    for event in events {
        if let Some(value_usage) = event.get("usage") {
            usage = Some(value_usage.clone());
        }

        let Some(choices) = event.get("choices").and_then(Value::as_array) else {
            continue;
        };

        for choice in choices {
            if let Some(reason) = choice
                .get("finish_reason")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|item| !item.is_empty())
            {
                finish_reason = Some(reason.to_string());
            }

            if let Some(delta) = choice.get("delta") {
                if let Some(text) = delta.get("content").and_then(Value::as_str) {
                    content.push_str(text);
                } else if let Some(parts) = delta.get("content").and_then(Value::as_array) {
                    for part in parts {
                        if let Some(text) = part.get("text").and_then(Value::as_str) {
                            content.push_str(text);
                        }
                    }
                }

                if let Some(items) = delta.get("tool_calls").and_then(Value::as_array) {
                    for item in items {
                        let index = item
                            .get("index")
                            .and_then(Value::as_u64)
                            .map(|value| value as usize)
                            .unwrap_or(tool_calls.len());
                        while tool_calls.len() <= index {
                            tool_calls.push(ToolCallAccumulator::default());
                        }
                        if let Some(id) = item
                            .get("id")
                            .and_then(Value::as_str)
                            .map(str::trim)
                            .filter(|value| !value.is_empty())
                        {
                            tool_calls[index].id = Some(id.to_string());
                        }
                        if let Some(function) = item.get("function") {
                            if let Some(name) = function
                                .get("name")
                                .and_then(Value::as_str)
                                .map(str::trim)
                                .filter(|value| !value.is_empty())
                            {
                                tool_calls[index].name = Some(name.to_string());
                            }
                            if let Some(arguments) =
                                function.get("arguments").and_then(Value::as_str)
                            {
                                tool_calls[index].arguments.push_str(arguments);
                            }
                        }
                    }
                }
            }
        }
    }

    let mut message = Map::new();
    if content.trim().is_empty() {
        message.insert("content".to_string(), Value::Null);
    } else {
        message.insert("content".to_string(), Value::String(content));
    }

    let normalized_tool_calls = tool_calls
        .iter()
        .enumerate()
        .filter_map(|(index, item)| {
            let name = item.name.as_deref()?.trim();
            if name.is_empty() {
                return None;
            }
            let id = item
                .id
                .clone()
                .unwrap_or_else(|| format!("call_{}", index + 1));
            let arguments = if item.arguments.trim().is_empty() {
                "{}".to_string()
            } else {
                item.arguments.clone()
            };
            Some(json!({
                "id": id,
                "type": "function",
                "function": {
                    "name": name,
                    "arguments": arguments,
                }
            }))
        })
        .collect::<Vec<_>>();

    if !normalized_tool_calls.is_empty() {
        message.insert(
            "tool_calls".to_string(),
            Value::Array(normalized_tool_calls),
        );
    }

    let mut out = json!({
        "choices": [
            {
                "message": Value::Object(message),
                "finish_reason": finish_reason.unwrap_or_else(|| "stop".to_string()),
            }
        ]
    });
    if let Some(value_usage) = usage {
        out["usage"] = value_usage;
    }
    Ok(out)
}

pub(super) fn aggregate_responses_stream(events: &[Value]) -> Result<Value, (StatusCode, String)> {
    let mut completed_response: Option<Value> = None;
    let mut response_template: Option<Value> = None;
    let mut output_items: Vec<Value> = Vec::new();
    let mut output_text = String::new();
    let mut reasoning_text = String::new();

    for event in events {
        if event.get("object").and_then(Value::as_str) == Some("response") {
            completed_response = Some(event.clone());
        }

        if let Some(response) = event.get("response") {
            response_template = Some(response.clone());
            let event_type = event
                .get("type")
                .and_then(Value::as_str)
                .unwrap_or_default();
            if event_type == "response.completed" || event_type == "response.failed" {
                completed_response = Some(response.clone());
            }
        }

        let event_type = event
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if event_type == "response.output_text.delta" {
            if let Some(delta) = event.get("delta").and_then(Value::as_str) {
                output_text.push_str(delta);
            }
        } else if event_type == "response.reasoning.delta" {
            if let Some(delta) = event.get("delta").and_then(Value::as_str) {
                reasoning_text.push_str(delta);
            }
        } else if event_type == "response.reasoning.done" {
            if let Some(text) = event.get("text").and_then(Value::as_str) {
                reasoning_text = text.to_string();
            }
        } else if event_type == "response.output_item.done" {
            if let Some(item) = event.get("item") {
                output_items.push(item.clone());
            }
        }
    }

    if let Some(response) = completed_response {
        return Ok(response);
    }

    let mut response = response_template
        .and_then(|value| value.as_object().cloned())
        .unwrap_or_default();

    if output_items.is_empty() && !output_text.trim().is_empty() {
        output_items.push(json!({
            "type": "message",
            "status": "completed",
            "role": "assistant",
            "content": [
                {
                    "type": "output_text",
                    "text": output_text.clone(),
                }
            ],
        }));
    }

    if !output_items.is_empty() {
        response.insert("output".to_string(), Value::Array(output_items));
    }
    if !output_text.trim().is_empty() {
        response.insert("output_text".to_string(), Value::String(output_text));
    }
    if !reasoning_text.trim().is_empty() {
        response.insert("reasoning".to_string(), Value::String(reasoning_text));
    }
    if !response.contains_key("status") {
        response.insert("status".to_string(), Value::String("completed".to_string()));
    }
    if !response.contains_key("object") {
        response.insert("object".to_string(), Value::String("response".to_string()));
    }

    if response.is_empty() {
        return Err(bad_gateway_error(
            "agent builder ai stream parse failed: no response payload assembled",
        ));
    }

    Ok(Value::Object(response))
}

pub(super) fn build_responses_input_from_messages(messages: &[Value]) -> Value {
    let mut items = Vec::new();

    for message in messages {
        let role = message
            .get("role")
            .and_then(Value::as_str)
            .map(str::trim)
            .unwrap_or("");
        if role.is_empty() {
            continue;
        }

        if role == "tool" {
            let call_id = message
                .get("tool_call_id")
                .and_then(Value::as_str)
                .map(str::trim)
                .unwrap_or("");
            if call_id.is_empty() {
                continue;
            }

            let raw_output = message.get("content").cloned().unwrap_or(Value::Null);
            let output = if let Some(text) = raw_output.as_str() {
                parse_json_candidate(text).unwrap_or_else(|| Value::String(text.to_string()))
            } else {
                raw_output
            };

            items.push(json!({
                "type": "function_call_output",
                "call_id": call_id,
                "output": output,
            }));
            continue;
        }

        if role == "assistant" {
            if let Some(tool_calls) = message.get("tool_calls").and_then(Value::as_array) {
                for call in tool_calls {
                    let call_id = call
                        .get("id")
                        .and_then(Value::as_str)
                        .map(str::trim)
                        .unwrap_or("");
                    let function = call.get("function");
                    let name = function
                        .and_then(|item| item.get("name"))
                        .and_then(Value::as_str)
                        .map(str::trim)
                        .unwrap_or("");
                    if call_id.is_empty() || name.is_empty() {
                        continue;
                    }

                    let arguments = function
                        .and_then(|item| item.get("arguments"))
                        .cloned()
                        .unwrap_or_else(|| Value::String("{}".to_string()));
                    let arguments = arguments
                        .as_str()
                        .map(|raw| raw.to_string())
                        .unwrap_or_else(|| arguments.to_string());

                    items.push(json!({
                        "type": "function_call",
                        "call_id": call_id,
                        "name": name,
                        "arguments": arguments,
                    }));
                }
            }
        }

        if let Some(text) = extract_message_text(message.get("content")) {
            let trimmed = text.trim();
            if !trimmed.is_empty() {
                items.push(json!({
                    "type": "message",
                    "role": role,
                    "content": [
                        {
                            "type": "input_text",
                            "text": trimmed,
                        }
                    ],
                }));
            }
        }
    }

    Value::Array(items)
}

pub(super) fn adapt_responses_to_chat_completion(value: Value) -> Value {
    let content = extract_responses_output_text(&value);
    let tool_calls = extract_responses_tool_calls(&value);
    let finish_reason = value
        .get("status")
        .and_then(Value::as_str)
        .map(|status| {
            if status == "completed" {
                "stop".to_string()
            } else {
                status.to_string()
            }
        })
        .unwrap_or_else(|| "stop".to_string());

    let mut message = Map::new();
    message.insert(
        "content".to_string(),
        content.map(Value::String).unwrap_or(Value::Null),
    );
    if !tool_calls.is_empty() {
        message.insert("tool_calls".to_string(), Value::Array(tool_calls));
    }

    json!({
        "choices": [
            {
                "message": Value::Object(message),
                "finish_reason": finish_reason,
            }
        ]
    })
}

pub(super) fn extract_responses_output_text(value: &Value) -> Option<String> {
    if let Some(text) = value.get("output_text").and_then(Value::as_str) {
        let trimmed = text.trim();
        if !trimmed.is_empty() {
            return Some(trimmed.to_string());
        }
    }

    let mut parts = Vec::new();
    let Some(items) = value.get("output").and_then(Value::as_array) else {
        return None;
    };

    for item in items {
        let item_type = item.get("type").and_then(Value::as_str).unwrap_or("");
        if item_type == "message" {
            if let Some(contents) = item.get("content").and_then(Value::as_array) {
                for content in contents {
                    let content_type = content.get("type").and_then(Value::as_str).unwrap_or("");
                    if content_type == "output_text"
                        || content_type == "input_text"
                        || content_type == "text"
                    {
                        if let Some(text) = content.get("text").and_then(Value::as_str) {
                            let trimmed = text.trim();
                            if !trimmed.is_empty() {
                                parts.push(trimmed.to_string());
                            }
                        }
                    }
                }
            }
            continue;
        }

        if (item_type == "output_text" || item_type == "input_text" || item_type == "text")
            && item.get("text").and_then(Value::as_str).is_some()
        {
            if let Some(text) = item.get("text").and_then(Value::as_str) {
                let trimmed = text.trim();
                if !trimmed.is_empty() {
                    parts.push(trimmed.to_string());
                }
            }
        }
    }

    if parts.is_empty() {
        None
    } else {
        Some(parts.join("\n"))
    }
}

pub(super) fn extract_responses_tool_calls(value: &Value) -> Vec<Value> {
    let Some(items) = value.get("output").and_then(Value::as_array) else {
        return Vec::new();
    };

    let mut out = Vec::new();
    for item in items {
        if item.get("type").and_then(Value::as_str) != Some("function_call") {
            continue;
        }

        let call_id = item
            .get("call_id")
            .and_then(Value::as_str)
            .or_else(|| item.get("id").and_then(Value::as_str))
            .map(str::trim)
            .unwrap_or("");
        let name = item
            .get("name")
            .and_then(Value::as_str)
            .map(str::trim)
            .unwrap_or("");
        if call_id.is_empty() || name.is_empty() {
            continue;
        }

        let arguments = item
            .get("arguments")
            .cloned()
            .unwrap_or_else(|| Value::String("{}".to_string()));
        let arguments = arguments
            .as_str()
            .map(|raw| raw.to_string())
            .unwrap_or_else(|| arguments.to_string());

        out.push(json!({
            "id": call_id,
            "type": "function",
            "function": {
                "name": name,
                "arguments": arguments,
            }
        }));
    }

    out
}

pub(super) fn parse_tool_calls(value: Option<&Value>) -> Vec<ToolCall> {
    let Some(items) = value.and_then(Value::as_array) else {
        return Vec::new();
    };

    items
        .iter()
        .filter_map(|item| {
            let id = item.get("id")?.as_str()?.trim().to_string();
            let function = item.get("function")?;
            let name = function.get("name")?.as_str()?.trim().to_string();
            if id.is_empty() || name.is_empty() {
                return None;
            }
            let arguments = match function.get("arguments") {
                Some(Value::String(raw)) => parse_json_candidate(raw).unwrap_or_else(|| json!({})),
                Some(other) => other.clone(),
                None => json!({}),
            };
            Some(ToolCall {
                id,
                name,
                arguments,
                raw: item.clone(),
            })
        })
        .collect()
}

pub(super) fn extract_message_text(value: Option<&Value>) -> Option<String> {
    let content = value?;
    match content {
        Value::String(text) => Some(text.trim().to_string()),
        Value::Array(items) => {
            let text = items
                .iter()
                .filter_map(|item| {
                    item.get("text")
                        .and_then(Value::as_str)
                        .or_else(|| item.get("content").and_then(Value::as_str))
                })
                .collect::<Vec<_>>()
                .join("\n");
            let trimmed = text.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        }
        _ => None,
    }
}
