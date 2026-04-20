use serde_json::Value;
use tracing::info;

use super::to_message_item;

fn tool_output_item_max_chars() -> usize {
    std::env::var("AI_V3_TOOL_OUTPUT_MAX_CHARS")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(12_000)
}

pub(super) fn cap_tool_output_for_input(raw: &str) -> String {
    truncate_text_preserve_head_and_tail(raw, tool_output_item_max_chars())
}

fn truncate_text_preserve_head_and_tail(raw: &str, max_chars: usize) -> String {
    if max_chars == 0 {
        return String::new();
    }

    let total = raw.chars().count();
    if total <= max_chars {
        return raw.to_string();
    }

    let marker = format!("\n[...truncated {} chars...]\n", total - max_chars);
    let marker_chars = marker.chars().count();
    if marker_chars >= max_chars {
        return raw.chars().take(max_chars).collect();
    }

    let keep_chars = max_chars - marker_chars;
    let keep_head = ((keep_chars * 2) / 5).max(1);
    let keep_tail = keep_chars.saturating_sub(keep_head);
    let head: String = raw.chars().take(keep_head).collect();
    let tail: String = raw
        .chars()
        .rev()
        .take(keep_tail)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    format!("{}{}{}", head, marker, tail)
}

pub(super) fn truncate_function_call_outputs_in_input(input: &Value) -> Option<Value> {
    let items = input.as_array()?;
    let mut changed = false;
    let mut mapped = Vec::with_capacity(items.len());

    for item in items {
        let mut cloned = item.clone();
        let is_output_item =
            cloned.get("type").and_then(|value| value.as_str()) == Some("function_call_output");
        if is_output_item {
            if let Some(object) = cloned.as_object_mut() {
                if let Some(raw) = object
                    .get("output")
                    .and_then(|value| value.as_str())
                    .map(|value| value.to_string())
                {
                    let truncated = cap_tool_output_for_input(raw.as_str());
                    if truncated != raw {
                        object.insert("output".to_string(), Value::String(truncated));
                        changed = true;
                    }
                }
            }
        }
        mapped.push(cloned);
    }

    if changed {
        Some(Value::Array(mapped))
    } else {
        None
    }
}

fn usage_value_i64(value: &Value, key: &str) -> Option<i64> {
    value
        .get(key)
        .and_then(|item| item.as_i64().or_else(|| item.as_u64().map(|v| v as i64)))
}

fn usage_nested_i64(value: &Value, parent: &str, key: &str) -> Option<i64> {
    value
        .get(parent)
        .and_then(|item| item.get(key))
        .and_then(|item| item.as_i64().or_else(|| item.as_u64().map(|v| v as i64)))
}

pub(super) fn log_usage_snapshot(purpose: &str, usage: Option<&Value>) {
    let Some(usage) = usage else {
        return;
    };
    let input_tokens = usage_value_i64(usage, "input_tokens")
        .or_else(|| usage_value_i64(usage, "prompt_tokens"))
        .unwrap_or(-1);
    let output_tokens = usage_value_i64(usage, "output_tokens")
        .or_else(|| usage_value_i64(usage, "completion_tokens"))
        .unwrap_or(-1);
    let cached_tokens = usage_nested_i64(usage, "input_tokens_details", "cached_tokens")
        .or_else(|| usage_nested_i64(usage, "prompt_tokens_details", "cached_tokens"))
        .unwrap_or(0);

    info!(
        "[AI_V3] usage snapshot: purpose={}, input_tokens={}, cached_tokens={}, output_tokens={}",
        purpose, input_tokens, cached_tokens, output_tokens
    );
}

pub(super) fn rewrite_system_messages_to_user(input: &Value, force_text_content: bool) -> Value {
    let Some(items) = input.as_array() else {
        return input.clone();
    };

    let mut changed = false;
    let mut mapped = Vec::with_capacity(items.len());

    for item in items {
        let item_type = item
            .get("type")
            .and_then(|value| value.as_str())
            .unwrap_or("");
        let role = item
            .get("role")
            .and_then(|value| value.as_str())
            .unwrap_or("");

        if item_type == "message" && (role == "system" || role == "developer") {
            let content = response_content_to_text(item.get("content").unwrap_or(&Value::Null));
            let label = if role == "developer" {
                "开发者上下文"
            } else {
                "系统上下文"
            };
            let wrapped = if content.trim().is_empty() {
                format!("【{}】", label)
            } else {
                format!("【{}】\n{}", label, content)
            };
            mapped.push(to_message_item(
                "user",
                &Value::String(wrapped),
                force_text_content,
            ));
            changed = true;
            continue;
        }

        mapped.push(item.clone());
    }

    if changed {
        Value::Array(mapped)
    } else {
        input.clone()
    }
}

fn response_content_to_text(content: &Value) -> String {
    if let Some(text) = content.as_str() {
        return text.to_string();
    }

    if let Some(array) = content.as_array() {
        let mut output = Vec::new();
        for part in array {
            if let Some(text) = part.as_str() {
                output.push(text.to_string());
                continue;
            }
            if let Some(text) = part.get("text").and_then(|value| value.as_str()) {
                output.push(text.to_string());
                continue;
            }
            if let Some(text) = part.get("output_text").and_then(|value| value.as_str()) {
                output.push(text.to_string());
                continue;
            }
            output.push(part.to_string());
        }
        return output.join("\n");
    }

    if let Some(object) = content.as_object() {
        if let Some(text) = object.get("text").and_then(|value| value.as_str()) {
            return text.to_string();
        }
        if let Some(text) = object.get("output").and_then(|value| value.as_str()) {
            return text.to_string();
        }
    }

    content.to_string()
}

#[cfg(test)]
mod tests {
    use serde_json::{json, Value};

    use super::{
        response_content_to_text, rewrite_system_messages_to_user,
        truncate_function_call_outputs_in_input,
    };

    #[test]
    fn rewrites_system_and_developer_messages_to_user_role() {
        let input = json!([
            {
                "type": "message",
                "role": "system",
                "content": [{"type":"input_text","text":"system prompt"}]
            },
            {
                "type": "message",
                "role": "developer",
                "content": [{"type":"input_text","text":"developer notes"}]
            },
            {
                "type": "message",
                "role": "user",
                "content": [{"type":"input_text","text":"hello"}]
            }
        ]);

        let output = rewrite_system_messages_to_user(&input, false);
        let arr = output.as_array().expect("array output");
        assert_eq!(arr.len(), 3);
        assert_eq!(
            arr[0].get("role").and_then(|value| value.as_str()),
            Some("user")
        );
        assert_eq!(
            arr[1].get("role").and_then(|value| value.as_str()),
            Some("user")
        );
        assert_eq!(
            arr[2].get("role").and_then(|value| value.as_str()),
            Some("user")
        );

        let first_text = response_content_to_text(arr[0].get("content").unwrap_or(&Value::Null));
        let second_text = response_content_to_text(arr[1].get("content").unwrap_or(&Value::Null));
        assert!(first_text.contains("系统上下文"));
        assert!(first_text.contains("system prompt"));
        assert!(second_text.contains("开发者上下文"));
        assert!(second_text.contains("developer notes"));
    }

    #[test]
    fn keeps_input_unchanged_when_no_system_messages_exist() {
        let input = json!([
            {
                "type": "message",
                "role": "user",
                "content": [{"type":"input_text","text":"hello"}]
            }
        ]);

        let output = rewrite_system_messages_to_user(&input, false);
        assert_eq!(input, output);
    }

    #[test]
    fn truncates_large_function_call_output_items() {
        let long_output = "a".repeat(20_000);
        let input = json!([
            {
                "type": "function_call_output",
                "call_id": "call_1",
                "output": long_output
            }
        ]);

        let truncated = truncate_function_call_outputs_in_input(&input).expect("should truncate");
        let items = truncated.as_array().expect("array");
        let text = items[0]
            .get("output")
            .and_then(|value| value.as_str())
            .unwrap_or("");
        assert!(text.len() < 20_000);
        assert!(text.contains("truncated"));
    }

    #[test]
    fn keeps_small_function_call_output_items_unchanged() {
        let input = json!([
            {
                "type": "function_call_output",
                "call_id": "call_2",
                "output": "small-output"
            }
        ]);
        let truncated = truncate_function_call_outputs_in_input(&input);
        assert!(truncated.is_none());
    }
}
