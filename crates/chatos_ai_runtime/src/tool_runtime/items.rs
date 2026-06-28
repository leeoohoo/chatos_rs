use std::collections::{HashMap, HashSet};

use chatos_mcp_runtime::ToolResult;
use serde_json::{json, Value};

use crate::tool_call::{
    build_function_call_item, build_function_call_output_item, clone_tool_call_arguments,
    extract_tool_call_id, extract_tool_call_name,
};

use super::budget::{sanitize_tool_results_for_model_with_budget, ToolResultModelBudgetLimits};

pub fn build_tool_call_items(tool_calls: &[Value]) -> Vec<Value> {
    let mut items = Vec::new();

    for tool_call in tool_calls {
        let call_id = extract_tool_call_id(tool_call).unwrap_or("").to_string();
        if call_id.is_empty() {
            continue;
        }

        let name = extract_tool_call_name(tool_call).unwrap_or("").to_string();
        let args = clone_tool_call_arguments(tool_call);
        let args_str = args
            .as_str()
            .map(|raw| raw.to_string())
            .unwrap_or_else(|| args.to_string());

        items.push(build_function_call_item(
            call_id.as_str(),
            name.as_str(),
            args_str.as_str(),
        ));
    }

    items
}

pub fn build_tool_output_items(results: &[ToolResult]) -> Vec<Value> {
    build_tool_output_items_with_budget(results, None)
}

pub fn build_tool_output_items_with_budget(
    results: &[ToolResult],
    limits: Option<ToolResultModelBudgetLimits>,
) -> Vec<Value> {
    let results = sanitize_tool_results_for_model_with_budget(results.to_vec(), limits);
    results
        .into_iter()
        .map(|result| {
            build_function_call_output_item(result.tool_call_id.as_str(), result.content.as_str())
        })
        .collect()
}

pub fn build_tool_output_items_for_calls_with_budget(
    tool_calls: &[Value],
    results: &[ToolResult],
    limits: Option<ToolResultModelBudgetLimits>,
) -> Vec<Value> {
    let tool_call_items = build_tool_call_items(tool_calls);
    let tool_output_items = build_tool_output_items_with_budget(results, limits);
    complete_tool_outputs_for_calls(tool_call_items.as_slice(), tool_output_items.as_slice())
}

pub fn append_tool_results(
    input: Value,
    supports_responses: bool,
    assistant_content: &str,
    tool_calls: &Value,
    results: Vec<ToolResult>,
) -> Value {
    append_tool_results_with_budget(
        input,
        supports_responses,
        assistant_content,
        tool_calls,
        results,
        None,
    )
}

pub fn append_tool_results_with_budget(
    input: Value,
    supports_responses: bool,
    assistant_content: &str,
    tool_calls: &Value,
    results: Vec<ToolResult>,
    limits: Option<ToolResultModelBudgetLimits>,
) -> Value {
    if supports_responses {
        return append_responses_tool_results_with_budget(input, tool_calls, results, limits);
    }

    let results = sanitize_tool_results_for_model_with_budget(results, limits);
    let mut items = input.as_array().cloned().unwrap_or_else(|| vec![input]);
    items.push(json!({
        "role": "assistant",
        "content": assistant_content,
        "tool_calls": tool_calls
    }));
    for result in results {
        items.push(json!({
            "role": "tool",
            "tool_call_id": result.tool_call_id,
            "content": result.content
        }));
    }
    Value::Array(items)
}

pub fn append_responses_tool_results(
    input: Value,
    tool_calls: &Value,
    results: Vec<ToolResult>,
) -> Value {
    append_responses_tool_results_with_budget(input, tool_calls, results, None)
}

pub fn append_responses_tool_results_with_budget(
    input: Value,
    tool_calls: &Value,
    results: Vec<ToolResult>,
    limits: Option<ToolResultModelBudgetLimits>,
) -> Value {
    let mut items = input.as_array().cloned().unwrap_or_else(|| vec![input]);
    let tool_call_items = tool_calls
        .as_array()
        .map(|calls| build_tool_call_items(calls.as_slice()))
        .unwrap_or_default();
    let tool_output_items = build_tool_output_items_with_budget(results.as_slice(), limits);
    let tool_output_items =
        complete_tool_outputs_for_calls(tool_call_items.as_slice(), tool_output_items.as_slice());
    items.extend(tool_call_items);
    items.extend(tool_output_items);
    Value::Array(items)
}

fn complete_tool_outputs_for_calls(
    tool_call_items: &[Value],
    tool_outputs: &[Value],
) -> Vec<Value> {
    let mut call_ids = Vec::new();
    let mut call_id_set = HashSet::new();
    let mut call_names = HashMap::new();

    for item in tool_call_items {
        let Some(call_id) = item.get("call_id").and_then(Value::as_str) else {
            continue;
        };
        if call_id.is_empty() {
            continue;
        }
        if call_id_set.insert(call_id.to_string()) {
            call_ids.push(call_id.to_string());
        }
        if let Some(name) = item.get("name").and_then(Value::as_str) {
            call_names.insert(call_id.to_string(), name.to_string());
        }
    }

    if call_ids.is_empty() {
        return Vec::new();
    }

    let mut output_ids = HashSet::new();
    let mut completed = Vec::new();
    for item in tool_outputs {
        let Some(call_id) = item.get("call_id").and_then(Value::as_str) else {
            continue;
        };
        if call_id.is_empty() || !call_id_set.contains(call_id) {
            continue;
        }
        if output_ids.insert(call_id.to_string()) {
            completed.push(item.clone());
        }
    }

    for call_id in call_ids {
        if output_ids.contains(call_id.as_str()) {
            continue;
        }
        let tool_name = call_names
            .get(call_id.as_str())
            .map(String::as_str)
            .unwrap_or("unknown");
        completed.push(build_function_call_output_item(
            call_id.as_str(),
            missing_tool_output_advisory(tool_name).as_str(),
        ));
    }

    completed
}

fn missing_tool_output_advisory(tool_name: &str) -> String {
    let tool_name = tool_name.trim();
    let tool_display = if tool_name.is_empty() {
        "unknown"
    } else {
        tool_name
    };
    format!(
        "[Tool result unavailable]\n\
Tool: {tool_display}\n\
Reason: the runtime did not receive a final output for this tool call. \
Treat this as a tool execution failure and continue with the available evidence, \
or retry the tool with narrower arguments if the missing result is required."
    )
}

pub fn merge_missing_tool_turn_items(
    items: &mut Vec<Value>,
    tool_call_items: &[Value],
    tool_outputs: &[Value],
) {
    let mut existing_call_ids: HashSet<String> = items
        .iter()
        .filter(|item| item.get("type").and_then(|value| value.as_str()) == Some("function_call"))
        .filter_map(|item| {
            item.get("call_id")
                .and_then(|value| value.as_str())
                .map(|value| value.to_string())
        })
        .collect();
    let mut pending_call_ids = HashSet::new();

    for item in tool_call_items {
        let Some(call_id) = item.get("call_id").and_then(|value| value.as_str()) else {
            continue;
        };
        if call_id.is_empty() {
            continue;
        }
        pending_call_ids.insert(call_id.to_string());
        if existing_call_ids.insert(call_id.to_string()) {
            items.push(item.clone());
        }
    }

    let mut existing_output_ids: HashSet<String> = items
        .iter()
        .filter(|item| {
            item.get("type").and_then(|value| value.as_str()) == Some("function_call_output")
        })
        .filter_map(|item| {
            item.get("call_id")
                .and_then(|value| value.as_str())
                .map(|value| value.to_string())
        })
        .collect();

    let tool_outputs = complete_tool_outputs_for_calls(tool_call_items, tool_outputs);
    for item in &tool_outputs {
        let Some(call_id) = item.get("call_id").and_then(|value| value.as_str()) else {
            continue;
        };
        if call_id.is_empty() || !pending_call_ids.contains(call_id) {
            continue;
        }
        if existing_output_ids.insert(call_id.to_string()) {
            items.push(item.clone());
        }
    }
}

pub fn merge_pending_tool_turn_items(
    items: &mut Vec<Value>,
    pending_tool_calls: Option<&[Value]>,
    pending_tool_outputs: Option<&[Value]>,
) {
    let tool_call_items = pending_tool_calls.unwrap_or(&[]);
    let tool_outputs = pending_tool_outputs.unwrap_or(&[]);
    merge_missing_tool_turn_items(items, tool_call_items, tool_outputs);
}

pub fn append_tool_turn_items(
    items: &mut Vec<Value>,
    assistant_item: Option<&Value>,
    tool_call_items: &[Value],
    tool_outputs: &[Value],
) {
    if let Some(item) = assistant_item {
        items.push(item.clone());
    }
    items.extend(tool_call_items.iter().cloned());
    items.extend(tool_outputs.iter().cloned());
}
