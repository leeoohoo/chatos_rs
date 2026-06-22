use serde_json::{json, Value};

use super::actions_shared::normalize_inline_text;

pub(super) fn build_console_messages_brief(messages: &[Value], max_items: usize) -> Vec<Value> {
    messages
        .iter()
        .take(max_items)
        .map(|item| {
            json!({
                "type": item.get("type").and_then(|v| v.as_str()).unwrap_or("log"),
                "text_preview": normalize_inline_text(
                    item.get("text").and_then(|v| v.as_str()).unwrap_or(""),
                    220
                ),
                "source": item.get("source").and_then(|v| v.as_str()).unwrap_or("console"),
            })
        })
        .collect()
}

pub(super) fn build_js_errors_brief(errors: &[Value], max_items: usize) -> Vec<Value> {
    errors
        .iter()
        .take(max_items)
        .map(|item| {
            json!({
                "message_preview": normalize_inline_text(
                    item.get("message").and_then(|v| v.as_str()).unwrap_or(""),
                    220
                ),
                "source": item.get("source").and_then(|v| v.as_str()).unwrap_or("exception"),
            })
        })
        .collect()
}

pub(super) fn build_console_message_counts(messages: &[Value]) -> Value {
    let mut counts = serde_json::Map::new();
    for item in messages {
        let key = item
            .get("type")
            .and_then(|v| v.as_str())
            .unwrap_or("log")
            .trim();
        if key.is_empty() {
            continue;
        }
        let next = counts
            .get(key)
            .and_then(|v| v.as_u64())
            .unwrap_or(0)
            .saturating_add(1);
        counts.insert(key.to_string(), Value::Number(next.into()));
    }
    Value::Object(counts)
}

pub(super) fn summarize_json_value_inline(value: &Value, max_chars: usize) -> String {
    match value {
        Value::Null => "null".to_string(),
        Value::Bool(flag) => flag.to_string(),
        Value::Number(number) => number.to_string(),
        Value::String(text) => normalize_inline_text(text, max_chars),
        Value::Array(items) => {
            if items.is_empty() {
                "empty array".to_string()
            } else {
                let item_types = items
                    .iter()
                    .take(3)
                    .map(result_type_name)
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("array({} items: {})", items.len(), item_types)
            }
        }
        Value::Object(map) => {
            if map.is_empty() {
                "empty object".to_string()
            } else {
                let keys = map.keys().take(5).cloned().collect::<Vec<_>>().join(", ");
                format!("object keys: {}", keys)
            }
        }
    }
}

pub(super) fn result_type_name(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "bool",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}
