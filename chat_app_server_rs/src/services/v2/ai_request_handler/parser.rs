use std::collections::BTreeMap;

use serde_json::{json, Value};

#[derive(Debug, Default)]
pub(super) struct StreamState {
    pub full_content: String,
    pub reasoning: String,
    pub tool_calls_map: BTreeMap<usize, Value>,
    pub finish_reason: Option<String>,
    pub usage: Option<Value>,
}

#[derive(Debug, Default)]
pub(super) struct StreamCallbacksPayload {
    pub chunk: Option<String>,
    pub thinking: Option<String>,
}

pub(super) fn normalize_reasoning_value(value: Option<&Value>) -> String {
    if let Some(value) = value {
        if let Some(text) = value.as_str() {
            return text.to_string();
        }
        if value.is_null() {
            return String::new();
        }
        if let Ok(text) = serde_json::to_string(value) {
            return text;
        }

        return value.to_string();
    }

    String::new()
}

pub(super) fn merge_tool_call_delta(
    tool_calls_map: &mut BTreeMap<usize, Value>,
    tool_call: &Value,
) {
    let index = tool_call
        .get("index")
        .and_then(|value| value.as_u64())
        .unwrap_or(0) as usize;

    let entry = tool_calls_map
        .entry(index)
        .or_insert(json!({"id":"","type":"function","function":{"name":"","arguments":""}}));

    if let Some(id) = tool_call.get("id").and_then(|value| value.as_str()) {
        entry["id"] = Value::String(id.to_string());
    }

    if let Some(function) = tool_call.get("function") {
        if let Some(name) = function.get("name").and_then(|value| value.as_str()) {
            let current = entry["function"]["name"].as_str().unwrap_or("").to_string();
            entry["function"]["name"] = Value::String(format!("{}{}", current, name));
        }

        if let Some(arguments) = function.get("arguments").and_then(|value| value.as_str()) {
            let current = entry["function"]["arguments"]
                .as_str()
                .unwrap_or("")
                .to_string();
            entry["function"]["arguments"] = Value::String(format!("{}{}", current, arguments));
        }
    }
}

pub(super) fn collect_tool_calls(tool_calls_map: &BTreeMap<usize, Value>) -> Option<Value> {
    if tool_calls_map.is_empty() {
        None
    } else {
        Some(Value::Array(tool_calls_map.values().cloned().collect()))
    }
}

pub(super) fn apply_stream_event(
    state: &mut StreamState,
    event: &Value,
    reasoning_enabled: bool,
) -> StreamCallbacksPayload {
    let mut payload = StreamCallbacksPayload::default();

    if let Some(usage) = event.get("usage") {
        state.usage = Some(usage.clone());
    }

    let choice = event.get("choices").and_then(|choices| choices.get(0));
    let Some(choice) = choice else {
        return payload;
    };

    if let Some(finish_reason) = choice.get("finish_reason").and_then(|value| value.as_str()) {
        state.finish_reason = Some(finish_reason.to_string());
    }

    let Some(delta) = choice.get("delta") else {
        return payload;
    };

    if let Some(content) = delta.get("content").and_then(|value| value.as_str()) {
        state.full_content.push_str(content);
        payload.chunk = Some(content.to_string());
    }

    if reasoning_enabled {
        let reasoning_piece = normalize_reasoning_value(
            delta
                .get("reasoning_content")
                .or_else(|| delta.get("reasoning")),
        );
        if !reasoning_piece.is_empty() {
            state.reasoning.push_str(&reasoning_piece);
            payload.thinking = Some(reasoning_piece);
        }
    }

    if let Some(tool_calls) = delta.get("tool_calls").and_then(|value| value.as_array()) {
        for tool_call in tool_calls {
            merge_tool_call_delta(&mut state.tool_calls_map, tool_call);
        }
    }

    payload
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_reasoning_value_handles_string_null_and_json() {
        assert_eq!(
            normalize_reasoning_value(Some(&Value::String("thinking".to_string()))),
            "thinking"
        );
        assert_eq!(normalize_reasoning_value(Some(&Value::Null)), "");
        assert_eq!(
            normalize_reasoning_value(Some(&json!({"step": 1}))),
            "{\"step\":1}"
        );
        assert_eq!(normalize_reasoning_value(None), "");
    }

    #[test]
    fn merge_tool_call_delta_appends_name_and_arguments_by_index() {
        let mut tool_calls_map = BTreeMap::new();

        merge_tool_call_delta(
            &mut tool_calls_map,
            &json!({
                "index": 0,
                "id": "call_1",
                "function": {
                    "name": "search",
                    "arguments": "{\"q\":"
                }
            }),
        );
        merge_tool_call_delta(
            &mut tool_calls_map,
            &json!({
                "index": 0,
                "function": {
                    "name": "_docs",
                    "arguments": "\"rust\"}"
                }
            }),
        );

        let calls = collect_tool_calls(&tool_calls_map)
            .and_then(|value| value.as_array().cloned())
            .unwrap_or_default();
        assert_eq!(calls.len(), 1);
        assert_eq!(
            calls[0].get("id").and_then(|value| value.as_str()),
            Some("call_1")
        );
        assert_eq!(
            calls[0]
                .get("function")
                .and_then(|value| value.get("name"))
                .and_then(|value| value.as_str()),
            Some("search_docs")
        );
        assert_eq!(
            calls[0]
                .get("function")
                .and_then(|value| value.get("arguments"))
                .and_then(|value| value.as_str()),
            Some("{\"q\":\"rust\"}")
        );
    }

    #[test]
    fn collect_tool_calls_returns_index_ordered_array() {
        let mut tool_calls_map = BTreeMap::new();
        tool_calls_map.insert(3, json!({"id": "call_3"}));
        tool_calls_map.insert(1, json!({"id": "call_1"}));

        let ids: Vec<String> = collect_tool_calls(&tool_calls_map)
            .and_then(|value| value.as_array().cloned())
            .unwrap_or_default()
            .iter()
            .filter_map(|item| item.get("id").and_then(|value| value.as_str()))
            .map(|value| value.to_string())
            .collect();

        assert_eq!(ids, vec!["call_1".to_string(), "call_3".to_string()]);
    }

    #[test]
    fn apply_stream_event_updates_state_and_emits_callbacks_payload() {
        let mut state = StreamState::default();
        let event = json!({
            "usage": {"prompt_tokens": 10},
            "choices": [{
                "finish_reason": "stop",
                "delta": {
                    "content": "hello",
                    "reasoning_content": "think",
                    "tool_calls": [{
                        "index": 0,
                        "id": "call_1",
                        "function": {"name": "tool", "arguments": "{}"}
                    }]
                }
            }]
        });

        let payload = apply_stream_event(&mut state, &event, true);

        assert_eq!(payload.chunk.as_deref(), Some("hello"));
        assert_eq!(payload.thinking.as_deref(), Some("think"));
        assert_eq!(state.full_content, "hello");
        assert_eq!(state.reasoning, "think");
        assert_eq!(state.finish_reason.as_deref(), Some("stop"));
        assert!(state.usage.is_some());
        assert_eq!(state.tool_calls_map.len(), 1);
    }
}
