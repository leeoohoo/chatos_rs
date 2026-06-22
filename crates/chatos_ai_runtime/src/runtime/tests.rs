use std::sync::Arc;

use serde_json::{json, Value};

use chatos_mcp_runtime::{ToolCallerModelRuntime, ToolResult};

use super::{
    append_runtime_input_items, empty_final_response_followup_item,
    merge_pending_tool_turn_into_input, should_persist_tool_result, IterativeContextRefresh,
    EMPTY_FINAL_RESPONSE_FOLLOWUP_PROMPT,
};
use crate::{AiRuntimeOptions, AiRuntimeResult, AiTurnReport, AiTurnStatus};

#[test]
fn runtime_options_pass_abort_checker_to_tool_context() {
    let options = AiRuntimeOptions::new(Some("session_1".to_string()), Some("turn_1".to_string()))
        .with_caller_model(Some("model_1".to_string()))
        .with_caller_model_runtime(Some(
            ToolCallerModelRuntime::openai_compatible(
                "https://example.com/v1",
                "secret",
                "model_1",
                "gpt",
            )
            .with_responses_support(true)
            .with_images_support(Some(true)),
        ))
        .with_abort_checker(Some(Arc::new(|session_id| session_id == "session_1")));

    assert!(options.is_aborted());
    let context = options.tool_call_context();
    assert_eq!(context.conversation_id.as_deref(), Some("session_1"));
    assert_eq!(context.conversation_turn_id.as_deref(), Some("turn_1"));
    assert_eq!(context.caller_model.as_deref(), Some("model_1"));
    let caller_runtime = context
        .caller_model_runtime
        .as_ref()
        .expect("caller runtime");
    assert_eq!(caller_runtime.model, "model_1");
    assert_eq!(caller_runtime.base_url, "https://example.com/v1");
    assert!(caller_runtime.supports_responses);
    assert_eq!(caller_runtime.supports_images, Some(true));
    assert!(context.is_aborted());
}

#[test]
fn turn_report_wraps_success_and_failure() {
    let report = AiRuntimeResult {
        content: "done".to_string(),
        reasoning: Some("because".to_string()),
        tool_calls: None,
        finish_reason: Some("stop".to_string()),
        usage: None,
        response_id: Some("resp_1".to_string()),
    }
    .into_report();

    assert_eq!(report.status, AiTurnStatus::Completed);
    assert!(report.is_completed());
    assert_eq!(report.content.as_deref(), Some("done"));
    assert_eq!(report.response_id.as_deref(), Some("resp_1"));

    let failed = AiTurnReport::failed("provider failed");
    assert_eq!(failed.status, AiTurnStatus::Failed);
    assert_eq!(failed.error.as_deref(), Some("provider failed"));

    let aborted = AiTurnReport::failed("aborted");
    assert_eq!(aborted.status, AiTurnStatus::Aborted);
    assert!(aborted.is_aborted());
    assert_eq!(aborted.user_message(), "任务已取消。");
    assert!(failed.user_message().contains("任务执行失败"));
    assert!(report.user_message().contains("done"));
}

#[tokio::test]
async fn iterative_context_refresh_composes_prefix_and_sticky_items() {
    let input = IterativeContextRefresh::new(
        None,
        None,
        vec![json!({"role":"system","content":"prefix"})],
    )
    .with_sticky_input_items(vec![json!({"role":"user","content":"current"})])
    .compose_input()
    .await
    .expect("iterative input");

    let items = input.as_array().expect("items");
    assert_eq!(items.len(), 2);
    assert_eq!(items[0]["content"].as_str(), Some("prefix"));
    assert_eq!(items[1]["content"].as_str(), Some("current"));
}

#[test]
fn merge_pending_tool_turn_into_input_repairs_refreshed_context() {
    let input = json!([
        {"type":"message","role":"user","content":[]},
        {"type":"function_call","call_id":"call_1","name":"search","arguments":"{}"}
    ]);
    let pending_calls =
        vec![json!({"type":"function_call","call_id":"call_1","name":"search","arguments":"{}"})];
    let pending_outputs =
        vec![json!({"type":"function_call_output","call_id":"call_1","output":"done"})];

    let merged = merge_pending_tool_turn_into_input(
        input,
        Some(pending_calls.as_slice()),
        Some(pending_outputs.as_slice()),
    );
    let items = merged.as_array().expect("items");

    assert_eq!(
        items
            .iter()
            .filter(|item| {
                item.get("type").and_then(|value| value.as_str()) == Some("function_call")
            })
            .count(),
        1
    );
    assert!(items.iter().any(|item| {
        item.get("type").and_then(|value| value.as_str()) == Some("function_call_output")
            && item.get("call_id").and_then(|value| value.as_str()) == Some("call_1")
    }));
}

#[test]
fn append_runtime_input_items_wraps_string_input_for_empty_final_followup() {
    let followup = empty_final_response_followup_item();
    let merged = append_runtime_input_items(Value::String("do the task".to_string()), &[followup]);
    let items = merged.as_array().expect("items");

    assert_eq!(items.len(), 2);
    assert_eq!(items[0]["role"].as_str(), Some("user"));
    assert_eq!(items[0]["content"].as_str(), Some("do the task"));
    assert_eq!(items[1]["role"].as_str(), Some("user"));
    assert_eq!(
        items[1]["content"].as_str(),
        Some(EMPTY_FINAL_RESPONSE_FOLLOWUP_PROMPT)
    );
}

#[test]
fn append_runtime_input_items_preserves_existing_items_for_empty_final_followup() {
    let followup = empty_final_response_followup_item();
    let merged = append_runtime_input_items(
        json!([
            {"role":"system","content":"rules"},
            {"role":"user","content":"run"}
        ]),
        &[followup],
    );
    let items = merged.as_array().expect("items");

    assert_eq!(items.len(), 3);
    assert_eq!(items[0]["role"].as_str(), Some("system"));
    assert_eq!(items[1]["role"].as_str(), Some("user"));
    assert_eq!(items[2]["role"].as_str(), Some("user"));
    assert_eq!(
        items[2]["content"].as_str(),
        Some(EMPTY_FINAL_RESPONSE_FOLLOWUP_PROMPT)
    );
}

#[test]
fn should_persist_tool_result_skips_successful_empty_arrays_only() {
    let empty_success = tool_result("[]", Some(json!([])), true, false, false);
    assert!(!should_persist_tool_result(&empty_success));

    let non_empty_success = tool_result("[1]", Some(json!([1])), true, false, false);
    assert!(should_persist_tool_result(&non_empty_success));

    let plain_text_brackets = tool_result("[]", None, true, false, false);
    assert!(should_persist_tool_result(&plain_text_brackets));

    let empty_error = tool_result("[]", Some(json!([])), false, true, false);
    assert!(should_persist_tool_result(&empty_error));

    let empty_stream = tool_result("[]", Some(json!([])), true, false, true);
    assert!(should_persist_tool_result(&empty_stream));
}

fn tool_result(
    content: &str,
    result: Option<Value>,
    success: bool,
    is_error: bool,
    is_stream: bool,
) -> ToolResult {
    ToolResult {
        tool_call_id: "call_1".to_string(),
        name: "task_runner_service_list_tasks".to_string(),
        success,
        is_error,
        is_stream,
        conversation_turn_id: Some("turn_1".to_string()),
        content: content.to_string(),
        result,
    }
}
