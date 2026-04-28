use std::collections::HashMap;

use serde_json::Value;

use crate::core::tool_call::{
    build_function_call_item, clone_tool_call_arguments, extract_tool_call_id,
    extract_tool_call_name, tool_call_arguments_text,
};
use crate::services::v3::mcp_tool_execute::ToolResult;

#[derive(Default)]
pub(super) struct ToolCallExecutionPlan {
    pub(super) display_calls: Vec<Value>,
    pub(super) execute_calls: Vec<Value>,
    pub(super) alias_map: HashMap<String, Vec<String>>,
}

pub(super) fn build_tool_call_execution_plan(tool_calls_arr: &[Value]) -> ToolCallExecutionPlan {
    let mut plan = ToolCallExecutionPlan::default();
    let mut exact_key_to_call_id: HashMap<String, String> = HashMap::new();

    for tool_call in tool_calls_arr {
        let call_id = extract_tool_call_id(tool_call)
            .unwrap_or("")
            .to_string();
        let tool_name = extract_tool_call_name(tool_call)
            .unwrap_or("")
            .to_string();
        let mut canonical_call_id: Option<String> = None;

        if canonical_call_id.is_none() && !call_id.is_empty() {
            let dedupe_key = format!(
                "{}::{}",
                tool_name.to_lowercase(),
                tool_call_arguments_text(tool_call)
            );
            if let Some(existing) = exact_key_to_call_id.get(&dedupe_key) {
                canonical_call_id = Some(existing.clone());
            } else {
                exact_key_to_call_id.insert(dedupe_key, call_id.clone());
            }
        }

        if let Some(existing) = canonical_call_id {
            if !call_id.is_empty() && call_id != existing {
                let entry = plan.alias_map.entry(existing).or_default();
                if !entry.iter().any(|id| id == &call_id) {
                    entry.push(call_id);
                }
            }
            continue;
        }

        plan.display_calls.push(tool_call.clone());
        plan.execute_calls.push(tool_call.clone());
    }

    plan
}

pub(super) fn expand_tool_results_with_aliases(
    tool_results: &[ToolResult],
    alias_map: &HashMap<String, Vec<String>>,
) -> Vec<ToolResult> {
    let mut expanded = Vec::new();

    for result in tool_results {
        expanded.push(result.clone());

        if let Some(alias_ids) = alias_map.get(result.tool_call_id.as_str()) {
            for alias_id in alias_ids {
                if alias_id.trim().is_empty() || alias_id == &result.tool_call_id {
                    continue;
                }
                let mut cloned = result.clone();
                cloned.tool_call_id = alias_id.clone();
                expanded.push(cloned);
            }
        }
    }

    expanded
}

pub(super) fn build_tool_call_items(tool_calls_arr: &[Value]) -> Vec<Value> {
    let mut items = Vec::new();

    for tool_call in tool_calls_arr {
        let call_id = extract_tool_call_id(tool_call)
            .unwrap_or("")
            .to_string();
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
