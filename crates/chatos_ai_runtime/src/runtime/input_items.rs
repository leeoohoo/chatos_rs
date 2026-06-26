use serde_json::{json, Value};

use crate::tool_runtime::merge_pending_tool_turn_items;

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
        map.insert("task_runner_debug".to_string(), runtime_debug.clone());
        payload
    } else {
        json!({
            "payload": payload,
            "task_runner_debug": runtime_debug,
        })
    }
}

pub(super) fn merge_pending_tool_turn_into_input(
    input: Value,
    pending_tool_calls: Option<&[Value]>,
    pending_tool_outputs: Option<&[Value]>,
) -> Value {
    if pending_tool_calls.is_none() && pending_tool_outputs.is_none() {
        return input;
    }

    let mut items = input.as_array().cloned().unwrap_or_else(|| {
        if input.is_null() {
            Vec::new()
        } else {
            vec![input]
        }
    });
    merge_pending_tool_turn_items(&mut items, pending_tool_calls, pending_tool_outputs);
    Value::Array(items)
}

pub(super) fn append_runtime_input_items(input: Value, items: &[Value]) -> Value {
    if items.is_empty() {
        return input;
    }
    let mut input_items = runtime_input_value_to_items(input);
    input_items.extend(items.iter().cloned());
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
