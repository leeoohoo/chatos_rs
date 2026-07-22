// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::{HashMap, HashSet};

use memory_engine_sdk::{ComposeContextPolicy, ComposeContextResponse, EngineRecord};
use serde_json::Value;

use crate::input_transform::to_message_item;
use crate::tool_call::{
    build_function_call_item, build_function_call_output_item, extract_message_tool_calls,
    extract_tool_call_id, extract_tool_call_name, tool_call_arguments_text,
};
use crate::tool_runtime::{ToolResultModelBudget, ToolResultModelBudgetLimits};

pub fn compose_response_to_input_items(response: &ComposeContextResponse) -> Vec<Value> {
    compose_response_to_input_items_with_budget(response, None)
}

pub fn compose_response_to_input_items_with_budget(
    response: &ComposeContextResponse,
    limits: Option<ToolResultModelBudgetLimits>,
) -> Vec<Value> {
    let mut items = Vec::new();
    let mut seen_tool_call_ids = HashSet::new();
    let mut remaining_tool_output_ids = collect_tool_output_id_counts(&response.recent_records);
    let sanitized_tool_outputs =
        sanitize_recent_tool_outputs(response.recent_records.as_slice(), limits);

    if !response.blocks.is_empty() {
        let text = response
            .blocks
            .iter()
            .map(|block| format!("[{}]\n{}", block.block_type, block.text))
            .collect::<Vec<_>>()
            .join("\n\n===\n\n");
        if !text.trim().is_empty() {
            items.push(to_message_item("system", &Value::String(text), false));
        }
    }

    for (record_index, record) in response.recent_records.iter().enumerate() {
        items.extend(engine_record_to_input_items(
            record,
            record_index,
            &mut seen_tool_call_ids,
            &mut remaining_tool_output_ids,
            &sanitized_tool_outputs,
        ));
    }

    items
}

pub(super) fn default_compose_policy() -> Option<ComposeContextPolicy> {
    Some(ComposeContextPolicy {
        include_recent_records: Some(true),
        include_thread_summary: Some(true),
        include_subject_memory: Some(true),
        recent_record_limit: None,
        summary_limit: None,
    })
}

fn engine_record_to_input_items(
    record: &EngineRecord,
    record_index: usize,
    seen_tool_call_ids: &mut HashSet<String>,
    remaining_tool_output_ids: &mut HashMap<String, usize>,
    sanitized_tool_outputs: &HashMap<usize, String>,
) -> Vec<Value> {
    let role = record.role.trim();
    if role.is_empty() {
        return Vec::new();
    }
    let mut items = Vec::new();

    if role == "tool" {
        if let Some(tool_call_id) = engine_record_tool_call_id(record) {
            if seen_tool_call_ids.contains(tool_call_id.as_str()) {
                let output = sanitized_tool_outputs
                    .get(&record_index)
                    .cloned()
                    .unwrap_or_else(|| record.content.clone());
                items.push(build_function_call_output_item(
                    tool_call_id.as_str(),
                    output.as_str(),
                ));
            }
            decrement_remaining_tool_output_id(remaining_tool_output_ids, tool_call_id.as_str());
        }
        return items;
    }

    if role == "assistant" {
        if !record.content.trim().is_empty() {
            items.push(to_message_item(
                "assistant",
                &Value::String(record.content.clone()),
                false,
            ));
        }
        for tool_call in
            extract_message_tool_calls(record.structured_payload.as_ref(), record.metadata.as_ref())
        {
            let Some(call_id) = extract_tool_call_id(&tool_call).map(str::trim) else {
                continue;
            };
            if call_id.is_empty() {
                continue;
            }
            let Some(name) = extract_tool_call_name(&tool_call).map(str::trim) else {
                continue;
            };
            if name.is_empty() {
                continue;
            }
            if !has_remaining_tool_output(remaining_tool_output_ids, call_id) {
                continue;
            }
            let arguments = tool_call_arguments_text(&tool_call);
            items.push(build_function_call_item(call_id, name, arguments.as_str()));
            seen_tool_call_ids.insert(call_id.to_string());
        }
        return items;
    }

    if matches!(role, "user" | "system" | "developer") && !record.content.trim().is_empty() {
        items.push(to_message_item(
            role,
            &Value::String(record.content.clone()),
            false,
        ));
    }
    items
}

fn sanitize_recent_tool_outputs(
    records: &[EngineRecord],
    limits: Option<ToolResultModelBudgetLimits>,
) -> HashMap<usize, String> {
    let tool_call_names = collect_tool_call_names(records);
    let mut budget = limits
        .map(ToolResultModelBudget::from_limits)
        .unwrap_or_else(ToolResultModelBudget::from_env);
    let mut outputs = HashMap::new();

    // Prefer the newest evidence when the cumulative history budget is full.
    // The model needs the latest state-changing result (for example an empty
    // TaskManager board) more than an older, already-consumed file dump.
    for (record_index, record) in records.iter().enumerate().rev() {
        if record.role.trim() != "tool" {
            continue;
        }
        let Some(tool_call_id) = engine_record_tool_call_id(record) else {
            continue;
        };
        let Some(call_name) = tool_call_names.get(tool_call_id.as_str()) else {
            continue;
        };
        let tool_name = record
            .metadata
            .as_ref()
            .and_then(|value| {
                value
                    .get("name")
                    .or_else(|| value.get("tool_name"))
                    .or_else(|| value.get("toolName"))
            })
            .and_then(Value::as_str)
            .unwrap_or(call_name.as_str());
        outputs.insert(
            record_index,
            budget.sanitize_content(tool_name, record.content.as_str()),
        );
    }

    outputs
}

fn collect_tool_call_names(records: &[EngineRecord]) -> HashMap<String, String> {
    let mut names = HashMap::new();
    for record in records {
        if record.role.trim() != "assistant" {
            continue;
        }
        for tool_call in
            extract_message_tool_calls(record.structured_payload.as_ref(), record.metadata.as_ref())
        {
            let Some(call_id) = extract_tool_call_id(&tool_call).map(str::trim) else {
                continue;
            };
            let Some(name) = extract_tool_call_name(&tool_call).map(str::trim) else {
                continue;
            };
            if !call_id.is_empty() && !name.is_empty() {
                names.insert(call_id.to_string(), name.to_string());
            }
        }
    }
    names
}

fn collect_tool_output_id_counts(records: &[EngineRecord]) -> HashMap<String, usize> {
    let mut counts = HashMap::new();
    for record in records {
        if record.role.trim() != "tool" {
            continue;
        }
        if let Some(tool_call_id) = engine_record_tool_call_id(record) {
            *counts.entry(tool_call_id).or_insert(0) += 1;
        }
    }
    counts
}

fn engine_record_tool_call_id(record: &EngineRecord) -> Option<String> {
    record
        .metadata
        .as_ref()
        .and_then(|value| {
            value
                .get("tool_call_id")
                .or_else(|| value.get("toolCallId"))
                .or_else(|| value.get("tool_callId"))
        })
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn has_remaining_tool_output(counts: &HashMap<String, usize>, call_id: &str) -> bool {
    counts.get(call_id).copied().unwrap_or_default() > 0
}

fn decrement_remaining_tool_output_id(counts: &mut HashMap<String, usize>, call_id: &str) {
    let should_remove = if let Some(count) = counts.get_mut(call_id) {
        *count = count.saturating_sub(1);
        *count == 0
    } else {
        false
    };
    if should_remove {
        counts.remove(call_id);
    }
}
