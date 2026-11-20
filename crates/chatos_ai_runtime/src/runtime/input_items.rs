// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde_json::{json, Value};

use super::EMPTY_FINAL_RESPONSE_FOLLOWUP_PROMPT;

pub(super) fn input_item_count(input: &Value) -> usize {
    input
        .as_array()
        .map(Vec::len)
        .unwrap_or(usize::from(!input.is_null()))
}

pub(super) fn json_value_size_bytes(value: &Value) -> usize {
    serde_json::to_vec(value)
        .map(|bytes| bytes.len())
        .unwrap_or_else(|_| value.to_string().len())
}

pub(super) fn attach_runtime_debug(mut payload: Value, runtime_debug: &Value) -> Value {
    if let Some(map) = payload.as_object_mut() {
        map.insert("task_execution_debug".to_string(), runtime_debug.clone());
        payload
    } else {
        json!({
            "payload": payload,
            "task_execution_debug": runtime_debug,
        })
    }
}

/// Merge runtime-owned sticky/lifecycle items without adding another identical
/// copy on every durable cloud step. This intentionally applies only at the
/// lifecycle boundary; user messages and tool history retain normal append
/// semantics even when their payloads happen to be equal.
pub(super) fn append_lifecycle_input_items(input: Value, items: &[Value]) -> Value {
    if items.is_empty() {
        return input;
    }
    let mut input_items = runtime_input_value_to_items(input);
    for item in items {
        if !input_items.iter().any(|existing| existing == item) {
            input_items.push(item.clone());
        }
    }
    Value::Array(input_items)
}

fn runtime_input_value_to_items(input: Value) -> Vec<Value> {
    match input {
        Value::Array(items) => items,
        Value::String(text) => vec![json!({"role": "user", "content": text})],
        Value::Null => Vec::new(),
        other => vec![json!({"role": "user", "content": other})],
    }
}

pub(super) fn empty_final_response_followup_item() -> Value {
    json!({
        "role": "user",
        "content": EMPTY_FINAL_RESPONSE_FOLLOWUP_PROMPT,
    })
}
