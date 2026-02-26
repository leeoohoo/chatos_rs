use std::collections::HashMap;

use serde_json::{json, Value};

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
    let mut first_suggest_call_id: Option<String> = None;

    for tool_call in tool_calls_arr {
        let call_id = tool_call_id(tool_call);
        let tool_name = tool_call_name(tool_call);
        let mut canonical_call_id: Option<String> = None;

        if is_suggest_sub_agent_tool(tool_name.as_str()) {
            if let Some(existing) = first_suggest_call_id.as_ref() {
                canonical_call_id = Some(existing.clone());
            }
        }

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

        if is_suggest_sub_agent_tool(tool_name.as_str())
            && !call_id.is_empty()
            && first_suggest_call_id.is_none()
        {
            first_suggest_call_id = Some(call_id);
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

fn tool_call_id(tool_call: &Value) -> String {
    tool_call
        .get("id")
        .and_then(|value| value.as_str())
        .or_else(|| tool_call.get("call_id").and_then(|value| value.as_str()))
        .unwrap_or("")
        .to_string()
}

fn tool_call_name(tool_call: &Value) -> String {
    tool_call
        .get("function")
        .and_then(|value| value.get("name"))
        .and_then(|value| value.as_str())
        .or_else(|| tool_call.get("name").and_then(|value| value.as_str()))
        .unwrap_or("")
        .to_string()
}

fn tool_call_arguments_text(tool_call: &Value) -> String {
    let arguments = tool_call
        .get("function")
        .and_then(|value| value.get("arguments"))
        .cloned()
        .or_else(|| tool_call.get("arguments").cloned())
        .unwrap_or(Value::String("{}".to_string()));

    if let Some(raw) = arguments.as_str() {
        return raw.trim().to_string();
    }

    arguments.to_string()
}

fn is_suggest_sub_agent_tool(tool_name: &str) -> bool {
    let normalized = tool_name.trim().to_lowercase();
    if normalized.is_empty() {
        return false;
    }

    normalized.ends_with("_suggest_sub_agent") || normalized.contains("__suggest_sub_agent")
}

pub(super) fn build_tool_call_items(tool_calls_arr: &[Value]) -> Vec<Value> {
    let mut items = Vec::new();

    for tool_call in tool_calls_arr {
        let call_id = tool_call
            .get("id")
            .and_then(|value| value.as_str())
            .or_else(|| tool_call.get("call_id").and_then(|value| value.as_str()))
            .unwrap_or("")
            .to_string();
        if call_id.is_empty() {
            continue;
        }

        let function = tool_call.get("function").cloned().unwrap_or(json!({}));
        let name = function
            .get("name")
            .and_then(|value| value.as_str())
            .or_else(|| tool_call.get("name").and_then(|value| value.as_str()))
            .unwrap_or("")
            .to_string();
        let args = function
            .get("arguments")
            .cloned()
            .or_else(|| tool_call.get("arguments").cloned())
            .unwrap_or(Value::String("{}".to_string()));
        let args_str = if let Some(raw) = args.as_str() {
            raw.to_string()
        } else {
            args.to_string()
        };

        items.push(json!({
            "type": "function_call",
            "call_id": call_id,
            "name": name,
            "arguments": args_str
        }));
    }

    items
}
