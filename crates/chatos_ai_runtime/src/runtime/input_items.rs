// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

#[cfg(feature = "local-agent-loop")]
use std::collections::HashMap;

use serde_json::{json, Value};

#[cfg(feature = "local-agent-loop")]
use crate::tool_runtime::{
    merge_pending_tool_turn_items, ToolResultModelBudget, ToolResultModelBudgetLimits,
};

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

pub(super) fn estimated_json_tokens(value: &Value) -> usize {
    json_value_size_bytes(value).saturating_add(3) / 4
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

#[cfg(feature = "local-agent-loop")]
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

#[cfg(feature = "local-agent-loop")]
pub(super) fn merge_current_turn_tool_history_into_input(
    input: Value,
    tool_call_items: &[Value],
    tool_output_items: &[Value],
    limits: Option<ToolResultModelBudgetLimits>,
) -> Value {
    if tool_call_items.is_empty() && tool_output_items.is_empty() {
        return input;
    }

    let mut items = input.as_array().cloned().unwrap_or_else(|| {
        if input.is_null() {
            Vec::new()
        } else {
            vec![input]
        }
    });
    let sanitized_outputs =
        sanitize_current_turn_tool_outputs(tool_call_items, tool_output_items, limits);
    merge_pending_tool_turn_items(
        &mut items,
        Some(tool_call_items),
        Some(sanitized_outputs.as_slice()),
    );
    Value::Array(items)
}

#[cfg(feature = "local-agent-loop")]
fn sanitize_current_turn_tool_outputs(
    tool_call_items: &[Value],
    tool_output_items: &[Value],
    limits: Option<ToolResultModelBudgetLimits>,
) -> Vec<Value> {
    let tool_names = tool_call_items
        .iter()
        .filter_map(|item| {
            let call_id = item.get("call_id")?.as_str()?.trim();
            let name = item.get("name")?.as_str()?.trim();
            (!call_id.is_empty() && !name.is_empty())
                .then(|| (call_id.to_string(), name.to_string()))
        })
        .collect::<HashMap<_, _>>();
    let mut budget = limits
        .map(ToolResultModelBudget::from_limits)
        .unwrap_or_else(ToolResultModelBudget::from_env);
    let mut sanitized = Vec::with_capacity(tool_output_items.len());

    // Keep the already-sent history prefix stable across iterations. The output
    // from the immediately preceding batch is merged afterwards as authoritative
    // pending evidence, so it can remain complete without rewriting older items.
    for item in tool_output_items {
        let mut item = item.clone();
        if item.get("type").and_then(Value::as_str) == Some("function_call_output") {
            let call_id = item
                .get("call_id")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let tool_name = tool_names
                .get(call_id)
                .map(String::as_str)
                .unwrap_or("unknown");
            if let Some(output) = item.get("output").and_then(Value::as_str) {
                item["output"] = Value::String(budget.sanitize_content(tool_name, output));
            }
        }
        sanitized.push(item);
    }
    sanitized
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
