use std::collections::HashMap;

use serde_json::{json, Value};

use chatos_mcp_runtime::ToolResult;

use crate::tool_call::{
    build_function_call_item, build_function_call_output_item, clone_tool_call_arguments,
    extract_tool_call_id, extract_tool_call_name, tool_call_arguments_text,
};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ToolCallExecutionPlan {
    pub display_calls: Vec<Value>,
    pub execute_calls: Vec<Value>,
    pub alias_map: HashMap<String, Vec<String>>,
}

pub fn build_tool_call_execution_plan(tool_calls: &[Value]) -> ToolCallExecutionPlan {
    let mut plan = ToolCallExecutionPlan::default();
    let mut exact_key_to_call_id: HashMap<String, String> = HashMap::new();

    for tool_call in tool_calls {
        let call_id = extract_tool_call_id(tool_call).unwrap_or("").to_string();
        let tool_name = extract_tool_call_name(tool_call).unwrap_or("").to_string();
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

pub fn expand_tool_results_with_aliases(
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

pub fn append_tool_results(
    input: Value,
    supports_responses: bool,
    assistant_content: &str,
    tool_calls: &Value,
    results: Vec<ToolResult>,
) -> Value {
    if supports_responses {
        return append_responses_tool_results(input, tool_calls, results);
    }

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
    let mut items = input.as_array().cloned().unwrap_or_else(|| vec![input]);
    if let Some(calls) = tool_calls.as_array() {
        items.extend(build_tool_call_items(calls.as_slice()));
    }
    for result in results {
        items.push(build_function_call_output_item(
            result.tool_call_id.as_str(),
            result.content.as_str(),
        ));
    }
    Value::Array(items)
}

pub fn merge_missing_tool_turn_items(
    items: &mut Vec<Value>,
    tool_call_items: &[Value],
    tool_outputs: &[Value],
) {
    let mut existing_call_ids: std::collections::HashSet<String> = items
        .iter()
        .filter(|item| item.get("type").and_then(|value| value.as_str()) == Some("function_call"))
        .filter_map(|item| {
            item.get("call_id")
                .and_then(|value| value.as_str())
                .map(|value| value.to_string())
        })
        .collect();
    let mut pending_call_ids = std::collections::HashSet::new();

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

    let mut existing_output_ids: std::collections::HashSet<String> = items
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

    for item in tool_outputs {
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

#[cfg(test)]
mod tests {
    use serde_json::{json, Value};

    use super::{
        append_tool_results, append_tool_turn_items, build_tool_call_execution_plan,
        build_tool_call_items, expand_tool_results_with_aliases, merge_missing_tool_turn_items,
        merge_pending_tool_turn_items,
    };

    #[test]
    fn tool_call_execution_plan_deduplicates_alias_calls() {
        let tool_calls = vec![
            json!({
                "id": "call_1",
                "function": {
                    "name": "search",
                    "arguments": "{\"q\":\"rust\"}"
                }
            }),
            json!({
                "id": "call_2",
                "function": {
                    "name": "search",
                    "arguments": "{\"q\":\"rust\"}"
                }
            }),
        ];

        let plan = build_tool_call_execution_plan(tool_calls.as_slice());
        assert_eq!(plan.display_calls.len(), 1);
        assert_eq!(plan.execute_calls.len(), 1);
        assert_eq!(
            plan.alias_map.get("call_1"),
            Some(&vec!["call_2".to_string()])
        );
    }

    #[test]
    fn build_tool_call_items_skips_entries_without_call_id() {
        let items = build_tool_call_items(&[
            json!({"id": "call_1", "function": {"name": "search", "arguments": "{}"}}),
            json!({"function": {"name": "search", "arguments": "{}"}}),
        ]);
        assert_eq!(items.len(), 1);
        assert_eq!(
            items[0].get("call_id").and_then(Value::as_str),
            Some("call_1")
        );
    }

    #[test]
    fn expand_tool_results_duplicates_results_for_alias_ids() {
        let results = vec![chatos_mcp_runtime::ToolResult {
            tool_call_id: "call_1".to_string(),
            name: "search".to_string(),
            success: true,
            is_error: false,
            is_stream: false,
            conversation_turn_id: None,
            content: "done".to_string(),
            result: None,
        }];
        let alias_map =
            std::collections::HashMap::from([("call_1".to_string(), vec!["call_2".to_string()])]);

        let expanded = expand_tool_results_with_aliases(results.as_slice(), &alias_map);
        assert_eq!(expanded.len(), 2);
        assert_eq!(expanded[1].tool_call_id, "call_2");
    }

    #[test]
    fn append_tool_results_supports_chat_and_responses_shapes() {
        let results = vec![chatos_mcp_runtime::ToolResult {
            tool_call_id: "call_1".to_string(),
            name: "search".to_string(),
            success: true,
            is_error: false,
            is_stream: false,
            conversation_turn_id: None,
            content: "done".to_string(),
            result: None,
        }];
        let tool_calls = json!([{
            "id": "call_1",
            "function": {
                "name": "search",
                "arguments": "{}"
            }
        }]);

        let chat_input = json!([{"role": "user", "content": "hello"}]);
        let chat_output =
            append_tool_results(chat_input, false, "working", &tool_calls, results.clone());
        assert_eq!(chat_output.as_array().map(Vec::len), Some(3));

        let responses_input = json!([{"type": "message", "role": "user", "content": []}]);
        let responses_output =
            append_tool_results(responses_input, true, "working", &tool_calls, results);
        assert_eq!(responses_output.as_array().map(Vec::len), Some(3));
        assert_eq!(
            responses_output
                .as_array()
                .and_then(|items| items.get(1))
                .and_then(|item| item.get("type"))
                .and_then(Value::as_str),
            Some("function_call")
        );
        assert_eq!(
            responses_output
                .as_array()
                .and_then(|items| items.last())
                .and_then(|item| item.get("type"))
                .and_then(Value::as_str),
            Some("function_call_output")
        );
    }

    #[test]
    fn merge_missing_tool_turn_items_deduplicates_and_keeps_matched_outputs() {
        let mut items = vec![
            json!({"type":"function_call","call_id":"call_1","name":"foo","arguments":"{}"}),
            json!({"type":"function_call_output","call_id":"call_1","output":"ok"}),
        ];
        let pending_calls = vec![
            json!({"type":"function_call","call_id":"call_1","name":"foo","arguments":"{}"}),
            json!({"type":"function_call","call_id":"call_2","name":"bar","arguments":"{}"}),
        ];
        let pending_outputs = vec![
            json!({"type":"function_call_output","call_id":"call_2","output":"done"}),
            json!({"type":"function_call_output","call_id":"call_3","output":"skip"}),
        ];

        merge_missing_tool_turn_items(
            &mut items,
            pending_calls.as_slice(),
            pending_outputs.as_slice(),
        );

        assert!(items.iter().any(|item| {
            item.get("type").and_then(Value::as_str) == Some("function_call")
                && item.get("call_id").and_then(Value::as_str) == Some("call_2")
        }));
        assert!(items.iter().any(|item| {
            item.get("type").and_then(Value::as_str) == Some("function_call_output")
                && item.get("call_id").and_then(Value::as_str) == Some("call_2")
        }));
        assert!(!items
            .iter()
            .any(|item| { item.get("call_id").and_then(Value::as_str) == Some("call_3") }));
    }

    #[test]
    fn merge_pending_tool_turn_items_skips_outputs_without_calls() {
        let mut items = Vec::new();
        let pending_outputs =
            vec![json!({"type":"function_call_output","call_id":"call_2","output":"done"})];

        merge_pending_tool_turn_items(&mut items, None, Some(pending_outputs.as_slice()));
        assert!(items.is_empty());
    }

    #[test]
    fn append_tool_turn_items_appends_assistant_then_tool_exchange() {
        let mut items = vec![json!({"type":"message","role":"user","content":[]})];
        let assistant = json!({"type":"message","role":"assistant","content":[]});
        let tool_calls = vec![json!({"type":"function_call","call_id":"call_1"})];
        let tool_outputs = vec![json!({"type":"function_call_output","call_id":"call_1"})];

        append_tool_turn_items(
            &mut items,
            Some(&assistant),
            tool_calls.as_slice(),
            tool_outputs.as_slice(),
        );

        assert_eq!(items.len(), 4);
        assert_eq!(
            items[1].get("role").and_then(Value::as_str),
            Some("assistant")
        );
        assert_eq!(
            items[2].get("type").and_then(Value::as_str),
            Some("function_call")
        );
        assert_eq!(
            items[3].get("type").and_then(Value::as_str),
            Some("function_call_output")
        );
    }
}
