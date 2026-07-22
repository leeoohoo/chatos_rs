// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde_json::{json, Value};

use super::{
    append_tool_results, append_tool_turn_items, build_tool_call_execution_plan,
    build_tool_call_items, build_tool_output_items, build_tool_output_items_for_calls_with_budget,
    expand_tool_results_with_aliases, merge_missing_tool_turn_items, merge_pending_tool_turn_items,
    sanitize_tool_results_for_model, sanitize_tool_results_for_model_with_budget,
    ToolResultModelBudgetLimits,
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
fn sanitize_tool_results_for_model_omits_large_content() {
    let results = vec![chatos_mcp_runtime::ToolResult {
        tool_call_id: "call_1".to_string(),
        name: "code.read_file".to_string(),
        success: true,
        is_error: false,
        is_stream: false,
        conversation_turn_id: None,
        content: "x".repeat(9_000),
        result: None,
    }];

    let sanitized = sanitize_tool_results_for_model(results);
    let content = sanitized[0].content.as_str();

    assert!(content.contains("Tool result omitted"));
    assert!(content.contains("code.read_file"));
    assert!(content.contains("read the file by line/range"));
    assert!(content.len() < 1_000);
}

#[test]
fn sanitize_tool_results_for_model_uses_explicit_budget_limits() {
    let results = vec![chatos_mcp_runtime::ToolResult {
        tool_call_id: "call_1".to_string(),
        name: "code.search".to_string(),
        success: true,
        is_error: false,
        is_stream: false,
        conversation_turn_id: None,
        content: "x".repeat(101),
        result: None,
    }];

    let sanitized = sanitize_tool_results_for_model_with_budget(
        results,
        Some(ToolResultModelBudgetLimits::new(100, 500)),
    );

    assert!(sanitized[0]
        .content
        .contains("single tool result exceeds the per-result model input limit"));
}

#[test]
fn build_tool_output_items_for_calls_fills_missing_outputs() {
    let tool_calls = vec![
        json!({"id": "call_1", "function": {"name": "process_poll", "arguments": "{}"}}),
        json!({"id": "call_2", "function": {"name": "process_poll", "arguments": "{}"}}),
    ];
    let results = vec![chatos_mcp_runtime::ToolResult {
        tool_call_id: "call_1".to_string(),
        name: "process_poll".to_string(),
        success: true,
        is_error: false,
        is_stream: false,
        conversation_turn_id: None,
        content: "done".to_string(),
        result: None,
    }];

    let outputs = build_tool_output_items_for_calls_with_budget(
        tool_calls.as_slice(),
        results.as_slice(),
        None,
    );

    assert_eq!(outputs.len(), 2);
    assert!(outputs.iter().any(|item| {
        item.get("call_id").and_then(Value::as_str) == Some("call_1")
            && item.get("output").and_then(Value::as_str) == Some("done")
    }));
    assert!(outputs.iter().any(|item| {
        item.get("call_id").and_then(Value::as_str) == Some("call_2")
            && item
                .get("output")
                .and_then(Value::as_str)
                .is_some_and(|output| output.contains("Tool result unavailable"))
    }));
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
fn merge_pending_tool_turn_items_replaces_budget_omission_with_latest_output() {
    let mut items = vec![
        json!({"type":"function_call","call_id":"call_1","name":"task_manager_list_tasks","arguments":"{}"}),
        json!({
            "type":"function_call_output",
            "call_id":"call_1",
            "output":"[Tool result omitted before sending to the model]"
        }),
    ];
    let pending_calls = vec![
        json!({"type":"function_call","call_id":"call_1","name":"task_manager_list_tasks","arguments":"{}"}),
    ];
    let pending_outputs = vec![json!({
        "type":"function_call_output",
        "call_id":"call_1",
        "output":"{\"count\":0,\"tasks\":[]}"
    })];

    merge_pending_tool_turn_items(
        &mut items,
        Some(pending_calls.as_slice()),
        Some(pending_outputs.as_slice()),
    );

    let outputs = items
        .iter()
        .filter(|item| {
            item.get("type").and_then(Value::as_str) == Some("function_call_output")
                && item.get("call_id").and_then(Value::as_str) == Some("call_1")
        })
        .collect::<Vec<_>>();
    assert_eq!(outputs.len(), 1);
    assert_eq!(
        outputs[0].get("output").and_then(Value::as_str),
        Some("{\"count\":0,\"tasks\":[]}")
    );
}

#[test]
fn merge_pending_tool_turn_items_fills_missing_outputs() {
    let mut items = Vec::new();
    let pending_calls =
        vec![json!({"type":"function_call","call_id":"call_1","name":"poll","arguments":"{}"})];

    merge_pending_tool_turn_items(&mut items, Some(pending_calls.as_slice()), None);

    assert_eq!(items.len(), 2);
    assert!(items.iter().any(|item| {
        item.get("type").and_then(Value::as_str) == Some("function_call_output")
            && item.get("call_id").and_then(Value::as_str) == Some("call_1")
            && item
                .get("output")
                .and_then(Value::as_str)
                .is_some_and(|output| output.contains("Tool result unavailable"))
    }));
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

#[test]
fn build_tool_output_items_sanitizes_large_content() {
    let results = vec![chatos_mcp_runtime::ToolResult {
        tool_call_id: "call_1".to_string(),
        name: "code.read_file".to_string(),
        success: true,
        is_error: false,
        is_stream: false,
        conversation_turn_id: None,
        content: "x".repeat(9_000),
        result: None,
    }];

    let items = build_tool_output_items(results.as_slice());
    let output = items[0]
        .get("output")
        .and_then(Value::as_str)
        .unwrap_or_default();

    assert!(output.contains("Tool result omitted"));
    assert!(output.contains("code.read_file"));
}
