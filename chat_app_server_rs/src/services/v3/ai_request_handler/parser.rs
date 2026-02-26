use serde_json::{json, Value};

#[derive(Debug, Default)]
pub(super) struct StreamState {
    pub full_content: String,
    pub reasoning: String,
    pub usage: Option<Value>,
    pub response_obj: Option<Value>,
    pub response_id: Option<String>,
    pub finish_reason: Option<String>,
    pub sent_any_chunk: bool,
}

#[derive(Debug, Default)]
pub(super) struct StreamCallbacksPayload {
    pub chunk: Option<String>,
    pub thinking: Option<String>,
}

pub(super) fn extract_tool_calls(response: &Value) -> Option<Value> {
    let mut tool_calls: Vec<Value> = Vec::new();

    if let Some(items) = response.get("output").and_then(|value| value.as_array()) {
        for item in items {
            if item.get("type").and_then(|value| value.as_str()) != Some("function_call") {
                continue;
            }

            let call_id = item
                .get("call_id")
                .and_then(|value| value.as_str())
                .or_else(|| item.get("id").and_then(|value| value.as_str()))
                .unwrap_or("");
            if call_id.is_empty() {
                continue;
            }

            let name = item
                .get("name")
                .and_then(|value| value.as_str())
                .unwrap_or("")
                .to_string();
            let arguments = item
                .get("arguments")
                .cloned()
                .unwrap_or(Value::String("{}".to_string()));
            let args_str = if let Some(raw) = arguments.as_str() {
                raw.to_string()
            } else {
                arguments.to_string()
            };

            tool_calls.push(json!({
                "id": call_id,
                "type": "function",
                "function": {
                    "name": name,
                    "arguments": args_str
                }
            }));
        }
    }

    if tool_calls.is_empty() {
        None
    } else {
        Some(Value::Array(tool_calls))
    }
}

pub(super) fn extract_output_text(response: &Value) -> String {
    if let Some(text) = response.get("output_text").and_then(|value| value.as_str()) {
        return text.to_string();
    }
    if let Some(text) = response.get("text").and_then(|value| value.as_str()) {
        return text.to_string();
    }

    if let Some(items) = response.get("output").and_then(|value| value.as_array()) {
        let mut text = String::new();

        for item in items {
            if item.get("type").and_then(|value| value.as_str()) != Some("message") {
                continue;
            }

            if let Some(content) = item.get("content").and_then(|value| value.as_str()) {
                text.push_str(content);
                continue;
            }

            if let Some(parts) = item.get("content").and_then(|value| value.as_array()) {
                for part in parts {
                    let part_type = part.get("type").and_then(|value| value.as_str());
                    if part_type == Some("output_text") || part_type == Some("text") {
                        if let Some(part_text) = part.get("text").and_then(|value| value.as_str()) {
                            text.push_str(part_text);
                        }
                    }
                }
            }
        }

        return text;
    }

    String::new()
}

pub(super) fn extract_text_delta(delta: &Value) -> Option<String> {
    if let Some(text) = delta.as_str() {
        return Some(text.to_string());
    }
    if let Some(text) = delta.get("text").and_then(|value| value.as_str()) {
        return Some(text.to_string());
    }
    if let Some(text) = delta.get("content").and_then(|value| value.as_str()) {
        return Some(text.to_string());
    }

    None
}

pub(super) fn extract_text_from_fields(value: &Value, fields: &[&str]) -> Option<String> {
    for key in fields {
        if let Some(inner) = value.get(*key) {
            if let Some(text) = extract_text_delta(inner) {
                return Some(text);
            }
        }
    }

    None
}

pub(super) fn extract_reasoning_event_text(event_type: &str, event: &Value) -> Option<String> {
    let is_reasoning_event = event_type.starts_with("response.reasoning")
        || event_type.starts_with("response.reasoning_text")
        || event_type.starts_with("response.reasoning_summary");

    if is_reasoning_event {
        for key in [
            "delta",
            "summary_text",
            "summary",
            "text",
            "part",
            "item",
            "content",
        ] {
            if let Some(value) = event.get(key) {
                let text = normalize_reasoning_delta(Some(value));
                if !text.is_empty() {
                    return Some(text);
                }
            }
        }

        if let Some(response) = event.get("response") {
            let text = extract_reasoning_from_response(response);
            if !text.is_empty() {
                return Some(text);
            }
        }
    }

    if event_type == "response.output_item.added"
        || event_type == "response.output_item.delta"
        || event_type == "response.output_item.done"
    {
        let item = event.get("item").or_else(|| event.get("output_item"));
        if let Some(item) = item {
            let item_type = item
                .get("type")
                .and_then(|value| value.as_str())
                .unwrap_or("");
            if item_type == "reasoning" || item_type == "reasoning_summary" {
                let text = extract_reasoning_from_response(&json!({ "output": [item.clone()] }));
                if !text.is_empty() {
                    return Some(text);
                }
            }
        }
    }

    None
}

pub(super) fn extract_reasoning_from_response(response: &Value) -> String {
    if let Some(reasoning) = response
        .get("reasoning")
        .or_else(|| response.get("reasoning_summary"))
    {
        let text = normalize_reasoning_delta(Some(reasoning));
        if !text.is_empty() {
            return text;
        }
    }

    let mut parts = Vec::new();
    if let Some(output_items) = response.get("output").and_then(|value| value.as_array()) {
        for item in output_items {
            let item_type = item
                .get("type")
                .and_then(|value| value.as_str())
                .unwrap_or("");
            if item_type != "reasoning" && item_type != "reasoning_summary" {
                continue;
            }

            let mut item_has_text = false;
            for key in ["summary", "content", "text", "delta", "reasoning"] {
                if let Some(value) = item.get(key) {
                    let text = normalize_reasoning_delta(Some(value));
                    if !text.is_empty() {
                        parts.push(text);
                        item_has_text = true;
                    }
                }
            }

            if !item_has_text {
                let text = normalize_reasoning_delta(Some(item));
                if !text.is_empty() {
                    parts.push(text);
                }
            }
        }
    }

    parts.join("")
}

fn extract_reasoning_text(value: &Value) -> String {
    if let Some(text) = value.as_str() {
        return text.to_string();
    }

    if let Some(array) = value.as_array() {
        let mut output = String::new();
        for item in array {
            let text = extract_reasoning_text(item);
            if !text.is_empty() {
                output.push_str(&text);
            }
        }
        return output;
    }

    let Some(object) = value.as_object() else {
        return String::new();
    };

    for key in [
        "text",
        "summary_text",
        "delta",
        "content",
        "summary",
        "reasoning",
        "reasoning_text",
        "value",
        "part",
        "item",
    ] {
        if let Some(inner) = object.get(key) {
            let text = extract_reasoning_text(inner);
            if !text.is_empty() {
                return text;
            }
        }
    }

    if let Some(parts) = object.get("content").and_then(|value| value.as_array()) {
        let mut output = String::new();
        for part in parts {
            let text = extract_reasoning_text(part);
            if !text.is_empty() {
                output.push_str(&text);
            }
        }
        return output;
    }

    String::new()
}

fn normalize_reasoning_delta(delta: Option<&Value>) -> String {
    delta.map(extract_reasoning_text).unwrap_or_default()
}

pub(super) fn looks_like_response_id(id: &str) -> bool {
    let normalized = id.trim().to_lowercase();
    if normalized.is_empty() {
        return false;
    }

    if normalized.starts_with("event_") || normalized.starts_with("call_") {
        return false;
    }

    if normalized.starts_with("resp_")
        || normalized.starts_with("response_")
        || normalized.starts_with("chatcmpl-")
        || normalized.starts_with("cmpl-")
    {
        return true;
    }

    normalized.len() >= 16
}

pub(super) fn apply_stream_event(state: &mut StreamState, event: &Value) -> StreamCallbacksPayload {
    let mut payload = StreamCallbacksPayload::default();

    if let Some(event_type) = event.get("type").and_then(|value| value.as_str()) {
        if event_type == "response.output_text.delta" {
            if let Some(delta) = event.get("delta").and_then(extract_text_delta) {
                if !delta.is_empty() {
                    state.full_content.push_str(&delta);
                    state.sent_any_chunk = true;
                    payload.chunk = Some(delta);
                }
            }
        } else if event_type == "response.output_text.done"
            || event_type == "response.output_text"
            || event_type == "response.output_text.completed"
        {
            if state.full_content.is_empty() {
                if let Some(text) =
                    extract_text_from_fields(event, &["text", "output_text", "delta"])
                {
                    if !text.is_empty() {
                        state.full_content.push_str(&text);
                        state.sent_any_chunk = true;
                        payload.chunk = Some(text);
                    }
                }
            }
        } else if let Some(reasoning_delta) = extract_reasoning_event_text(event_type, event) {
            if !reasoning_delta.is_empty() {
                state.reasoning.push_str(&reasoning_delta);
                payload.thinking = Some(reasoning_delta);
            }
        } else if event_type == "response.completed" {
            if let Some(response) = event.get("response") {
                state.response_obj = Some(response.clone());
                if state.full_content.is_empty() {
                    let extracted = extract_output_text(response);
                    if !extracted.is_empty() {
                        state.full_content.push_str(&extracted);
                        state.sent_any_chunk = true;
                        payload.chunk = Some(extracted);
                    }
                }
            } else {
                state.response_obj = Some(event.clone());
                if state.full_content.is_empty() {
                    let extracted = extract_output_text(event);
                    if !extracted.is_empty() {
                        state.full_content.push_str(&extracted);
                        state.sent_any_chunk = true;
                        payload.chunk = Some(extracted);
                    }
                }
            }
        } else if event_type == "response.failed" {
            if let Some(response) = event.get("response") {
                state.response_obj = Some(response.clone());
            }
        } else if state.response_obj.is_none() {
            if let Some(response) = event.get("response") {
                if response.get("output").is_some()
                    || response.get("output_text").is_some()
                    || response.get("status").is_some()
                {
                    state.response_obj = Some(response.clone());
                }
            } else if event.get("output").is_some() || event.get("output_text").is_some() {
                state.response_obj = Some(event.clone());
            }
        }
    }

    if let Some(id) = event
        .get("response")
        .and_then(|response| response.get("id"))
        .and_then(|value| value.as_str())
    {
        state.response_id = Some(id.to_string());
    } else if state.response_id.is_none() {
        if let Some(id) = event.get("id").and_then(|value| value.as_str()) {
            if looks_like_response_id(id) {
                state.response_id = Some(id.to_string());
            }
        }
    }

    if let Some(usage) = event
        .get("response")
        .and_then(|response| response.get("usage"))
    {
        state.usage = Some(usage.clone());
    }

    payload
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_tool_calls_collects_function_call_output_items() {
        let response = json!({
            "output": [
                {
                    "type": "function_call",
                    "call_id": "call_1",
                    "name": "mcp.search",
                    "arguments": {"q": "rust"}
                },
                {
                    "type": "function_call",
                    "id": "call_2",
                    "name": "mcp.read",
                    "arguments": "{\"path\":\"README.md\"}"
                },
                {
                    "type": "message",
                    "content": "ignored"
                }
            ]
        });

        let tool_calls = extract_tool_calls(&response)
            .and_then(|value| value.as_array().cloned())
            .unwrap_or_default();

        assert_eq!(tool_calls.len(), 2);
        assert_eq!(
            tool_calls[0]
                .get("function")
                .and_then(|value| value.get("arguments"))
                .and_then(|value| value.as_str()),
            Some("{\"q\":\"rust\"}")
        );
        assert_eq!(
            tool_calls[1].get("id").and_then(|value| value.as_str()),
            Some("call_2")
        );
    }

    #[test]
    fn extract_output_text_reads_output_message_parts() {
        let response = json!({
            "output": [
                {
                    "type": "message",
                    "content": [
                        {"type": "output_text", "text": "Hello"},
                        {"type": "text", "text": " world"}
                    ]
                }
            ]
        });

        assert_eq!(extract_output_text(&response), "Hello world");
    }

    #[test]
    fn extract_reasoning_from_response_collects_reasoning_summary_items() {
        let response = json!({
            "output": [
                {
                    "type": "reasoning",
                    "summary": [{"text": "step-1"}]
                },
                {
                    "type": "reasoning_summary",
                    "text": "step-2"
                }
            ]
        });

        assert_eq!(extract_reasoning_from_response(&response), "step-1step-2");
    }

    #[test]
    fn looks_like_response_id_filters_event_ids_and_accepts_response_prefix() {
        assert!(looks_like_response_id("resp_abc"));
        assert!(looks_like_response_id("chatcmpl-123"));
        assert!(!looks_like_response_id("event_123"));
        assert!(!looks_like_response_id("call_123"));
        assert!(!looks_like_response_id("   "));
    }

    #[test]
    fn apply_stream_event_updates_stream_state_and_payload() {
        let mut state = StreamState::default();
        let event = json!({
            "type": "response.output_text.delta",
            "delta": {"text": "hello"},
            "response": {
                "id": "resp_1",
                "usage": {"total_tokens": 9}
            }
        });

        let payload = apply_stream_event(&mut state, &event);

        assert_eq!(payload.chunk.as_deref(), Some("hello"));
        assert_eq!(payload.thinking, None);
        assert_eq!(state.full_content, "hello");
        assert!(state.sent_any_chunk);
        assert_eq!(state.response_id.as_deref(), Some("resp_1"));
        assert!(state.usage.is_some());
    }
}
