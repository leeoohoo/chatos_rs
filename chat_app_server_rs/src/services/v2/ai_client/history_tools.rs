use std::collections::HashSet;

use serde_json::{json, Value};

pub(super) fn normalize_content(content: &Value) -> String {
    if let Some(value) = content.as_str() {
        return value.to_string();
    }

    if let Some(array) = content.as_array() {
        for part in array {
            if part.get("type").and_then(|value| value.as_str()) == Some("text") {
                if let Some(text) = part.get("text").and_then(|value| value.as_str()) {
                    return text.to_string();
                }
            }
        }
        return String::new();
    }

    content.to_string()
}

pub(super) fn drop_duplicate_tail(history: Vec<Value>, current: &[Value]) -> Vec<Value> {
    if history.is_empty() || current.is_empty() {
        return history;
    }

    let mut history_index = history.len() as i64 - 1;
    let mut current_index = current.len() as i64 - 1;

    while history_index >= 0 && current_index >= 0 {
        let history_item = &history[history_index as usize];
        let current_item = &current[current_index as usize];
        if history_item.get("role") != current_item.get("role") {
            break;
        }

        let history_content =
            normalize_content(history_item.get("content").unwrap_or(&Value::Null));
        let current_content =
            normalize_content(current_item.get("content").unwrap_or(&Value::Null));
        if history_content != current_content {
            break;
        }

        history_index -= 1;
        current_index -= 1;
    }

    if current_index < (current.len() as i64 - 1) {
        if history_index < 0 {
            return Vec::new();
        }
        return history[..=(history_index as usize)].to_vec();
    }

    history
}

pub(super) fn ensure_tool_responses(history: Vec<Value>) -> Vec<Value> {
    let mut output = Vec::new();
    let mut index = 0usize;

    while index < history.len() {
        let message = history[index].clone();
        if message.get("role").and_then(|value| value.as_str()) == Some("tool") {
            index += 1;
            continue;
        }

        output.push(message.clone());
        if message.get("role").and_then(|value| value.as_str()) == Some("assistant") {
            let tool_calls = message
                .get("tool_calls")
                .and_then(|value| value.as_array())
                .cloned()
                .unwrap_or_default();

            if !tool_calls.is_empty() {
                let expected_ids: Vec<String> = tool_calls
                    .iter()
                    .filter_map(|tool_call| {
                        tool_call
                            .get("id")
                            .and_then(|value| value.as_str())
                            .map(|value| value.to_string())
                    })
                    .collect();
                let mut present_ids = HashSet::new();
                let mut next_index = index + 1;

                while next_index < history.len() {
                    let next = &history[next_index];
                    if next.get("role").and_then(|value| value.as_str()) != Some("tool") {
                        break;
                    }
                    if let Some(id) = next.get("tool_call_id").and_then(|value| value.as_str()) {
                        present_ids.insert(id.to_string());
                    }
                    output.push(next.clone());
                    next_index += 1;
                }

                for id in expected_ids {
                    if !present_ids.contains(&id) {
                        output.push(
                            json!({"role": "tool", "tool_call_id": id, "content": "aborted"}),
                        );
                    }
                }

                index = next_index;
                continue;
            }
        }

        index += 1;
    }

    output
}

pub(super) fn find_summary_index(messages: &[Value], summary_prompt: Option<&String>) -> i64 {
    if summary_prompt.is_none() {
        return -1;
    }

    let summary_prompt = summary_prompt.unwrap();
    for (index, message) in messages.iter().enumerate().rev() {
        if message.get("role").and_then(|value| value.as_str()) == Some("system") {
            if let Some(content) = message.get("content").and_then(|value| value.as_str()) {
                if content == summary_prompt {
                    return index as i64;
                }
            }
        }
    }

    -1
}

pub(super) fn find_anchor_index(messages: &[Value], anchor: Option<&Value>) -> i64 {
    let anchor = match anchor {
        Some(value) => value,
        None => return -1,
    };

    for (index, message) in messages.iter().enumerate().rev() {
        if message.get("role").and_then(|value| value.as_str()) == Some("user") {
            let content = message.get("content").unwrap_or(&Value::Null);
            if content == anchor {
                return index as i64;
            }
        }
    }

    -1
}
