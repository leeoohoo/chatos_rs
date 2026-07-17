// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use axum::{extract::State, http::StatusCode, routing::post, Json, Router};
use serde_json::{json, Value};
use tokio::sync::Mutex as AsyncMutex;

use chatos_mcp_runtime::{ToolCallerModelRuntime, ToolResult};

use super::{
    append_runtime_input_items, empty_final_response_followup_item,
    merge_pending_tool_turn_into_input, merge_record_metadata, prepare_iteration_request,
    should_persist_tool_result, IterativeContextRefresh, EMPTY_FINAL_RESPONSE_FOLLOWUP_PROMPT,
};
use crate::{
    AiResponse, AiRuntime, AiRuntimeOptions, AiRuntimeResult, AiTurnReport, AiTurnStatus,
    ModelRequest, RuntimeBeforeModelRequest, RuntimeFinalResponseAction,
    RuntimeFinalResponseContext, RuntimeIterationContext, RuntimeLifecycleHook,
};

struct TestLifecycleHook;

#[async_trait]
impl RuntimeLifecycleHook for TestLifecycleHook {
    async fn before_model_request(
        &self,
        _context: RuntimeIterationContext,
    ) -> Result<RuntimeBeforeModelRequest, String> {
        Ok(RuntimeBeforeModelRequest::unchanged()
            .with_input_items(vec![json!({"role": "system", "content": "dynamic"})])
            .with_stream_output(false)
            .with_tools_enabled(false))
    }
}

#[tokio::test]
async fn lifecycle_hook_builds_ephemeral_iteration_request() {
    let request = ModelRequest::openai_compatible(
        "http://localhost",
        "key",
        "model",
        "openai_compatible",
        json!([{"role": "user", "content": "hello"}]),
    )
    .with_tools(vec![json!({"name": "tool"})]);
    let options = AiRuntimeOptions::for_conversation("session-1")
        .with_lifecycle_hook(Some(Arc::new(TestLifecycleHook)));

    let (iteration_request, directive) =
        prepare_iteration_request(&request, &options, 1, "initial")
            .await
            .expect("iteration request");

    assert_eq!(request.input.as_array().expect("base input").len(), 1);
    assert_eq!(iteration_request.input.as_array().expect("input").len(), 2);
    assert!(iteration_request.tools.is_empty());
    assert!(!directive.stream_output);
}

#[test]
fn lifecycle_record_metadata_overlays_static_record_metadata() {
    let merged = merge_record_metadata(
        Some(json!({"message_mode": "chat", "shared": "base"})),
        Some(json!({"task_turn_review": {"outcome": "pass"}, "shared": "hook"})),
    )
    .expect("merged metadata");

    assert_eq!(merged["message_mode"], "chat");
    assert_eq!(merged["shared"], "hook");
    assert_eq!(merged["task_turn_review"]["outcome"], "pass");
}

#[derive(Clone)]
struct MockLifecycleProviderState {
    responses: Arc<AsyncMutex<VecDeque<Value>>>,
    requests: Arc<AsyncMutex<Vec<Value>>>,
}

async fn mock_lifecycle_provider(
    State(state): State<MockLifecycleProviderState>,
    Json(payload): Json<Value>,
) -> (StatusCode, Json<Value>) {
    state.requests.lock().await.push(payload);
    let response = state.responses.lock().await.pop_front().unwrap_or_else(|| {
        json!({
            "id": "response-default",
            "status": "completed",
            "output_text": "ok"
        })
    });
    (StatusCode::OK, Json(response))
}

async fn start_lifecycle_mock_provider(
    responses: Vec<Value>,
) -> (
    String,
    Arc<AsyncMutex<Vec<Value>>>,
    tokio::task::JoinHandle<()>,
) {
    let state = MockLifecycleProviderState {
        responses: Arc::new(AsyncMutex::new(responses.into_iter().collect())),
        requests: Arc::new(AsyncMutex::new(Vec::new())),
    };
    let requests = Arc::clone(&state.requests);
    let app = Router::new()
        .route("/responses", post(mock_lifecycle_provider))
        .with_state(state);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind lifecycle mock provider");
    let address = listener.local_addr().expect("mock provider address");
    let server = tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    (format!("http://{address}"), requests, server)
}

#[derive(Default)]
struct ReviewLifecycleHook {
    visible_response: Mutex<Option<AiResponse>>,
}

#[async_trait]
impl RuntimeLifecycleHook for ReviewLifecycleHook {
    async fn before_model_request(
        &self,
        context: RuntimeIterationContext,
    ) -> Result<RuntimeBeforeModelRequest, String> {
        Ok(RuntimeBeforeModelRequest::unchanged()
            .with_stream_output(context.reason != "task_review")
            .with_tools_enabled(context.reason != "task_review"))
    }

    async fn after_final_response(
        &self,
        context: RuntimeFinalResponseContext,
    ) -> Result<RuntimeFinalResponseAction, String> {
        if context.reason == "task_review" {
            let visible = self
                .visible_response
                .lock()
                .map_err(|_| "visible response lock poisoned".to_string())?
                .clone()
                .unwrap_or(context.response);
            return Ok(RuntimeFinalResponseAction::Replace(Box::new(visible)));
        }

        *self
            .visible_response
            .lock()
            .map_err(|_| "visible response lock poisoned".to_string())? =
            Some(context.response.clone());
        Ok(RuntimeFinalResponseAction::Continue {
            input_items: vec![
                json!({
                    "type": "message",
                    "role": "assistant",
                    "content": [{
                        "type": "output_text",
                        "text": context.response.content
                    }]
                }),
                json!({
                    "type": "message",
                    "role": "system",
                    "content": [{
                        "type": "input_text",
                        "text": "Review the completed work and return TASK_REVIEW: pass."
                    }]
                }),
            ],
            reason: "task_review".to_string(),
        })
    }
}

#[tokio::test]
async fn lifecycle_continuation_runs_hidden_review_and_restores_visible_response() {
    let (base_url, requests, server) = start_lifecycle_mock_provider(vec![
        json!({
            "id": "response-visible",
            "status": "completed",
            "output_text": "visible summary"
        }),
        json!({
            "id": "response-review",
            "status": "completed",
            "output_text": "TASK_REVIEW: pass\nlooks good"
        }),
    ])
    .await;
    let request = ModelRequest::openai_compatible(
        base_url,
        "test-key",
        "gpt-test",
        "openai",
        json!([{"role": "user", "content": "complete the task"}]),
    )
    .with_responses_support(true)
    .with_tools(vec![json!({
        "type": "function",
        "name": "test_tool",
        "description": "test tool",
        "parameters": {"type": "object", "properties": {}}
    })]);
    let options = AiRuntimeOptions::for_conversation("session-1")
        .with_lifecycle_hook(Some(Arc::new(ReviewLifecycleHook::default())));

    let result = AiRuntime::new(None)
        .with_max_iterations(4)
        .run_turn(request, options)
        .await
        .expect("lifecycle review turn");
    server.abort();

    assert_eq!(result.content, "visible summary");
    let captured = requests.lock().await.clone();
    assert_eq!(captured.len(), 2);
    assert!(captured[0]
        .get("tools")
        .and_then(Value::as_array)
        .is_some_and(|tools| !tools.is_empty()));
    assert!(captured[1]
        .get("tools")
        .and_then(Value::as_array)
        .is_none_or(Vec::is_empty));
    assert!(captured[1].to_string().contains("visible summary"));
    assert!(captured[1].to_string().contains("TASK_REVIEW: pass"));
    assert!(captured
        .iter()
        .all(|payload| payload.get("prev_id").is_none()));
}

#[tokio::test]
async fn failed_provider_response_retries_five_times_before_succeeding() {
    let failed_response = json!({
        "id": "response-failed",
        "status": "failed",
        "error": null
    });
    let (base_url, requests, server) = start_lifecycle_mock_provider(vec![
        failed_response.clone(),
        failed_response.clone(),
        failed_response.clone(),
        failed_response.clone(),
        failed_response,
        json!({
            "id": "response-success",
            "status": "completed",
            "output_text": "completed after retries"
        }),
    ])
    .await;
    let request = ModelRequest::openai_compatible(
        base_url,
        "test-key",
        "gpt-test",
        "openai",
        json!([{"role": "user", "content": "complete the task"}]),
    )
    .with_responses_support(true);

    let result = AiRuntime::new(None)
        .with_max_iterations(2)
        .run_turn(request, AiRuntimeOptions::for_conversation("session-retry"))
        .await
        .expect("fifth retry should succeed");
    server.abort();

    assert_eq!(result.content, "completed after retries");
    assert_eq!(requests.lock().await.len(), 6);
}

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
fn runtime_options_abort_token_cancels_runtime_and_tool_context() {
    let token = tokio_util::sync::CancellationToken::new();
    let options =
        AiRuntimeOptions::for_conversation("session-token").with_abort_token(Some(token.clone()));

    assert!(!options.is_aborted());
    assert!(!options.tool_call_context().is_aborted());
    token.cancel();
    assert!(options.is_aborted());
    assert!(options.tool_call_context().is_aborted());
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
fn empty_final_followup_does_not_forbid_needed_tools() {
    assert!(!EMPTY_FINAL_RESPONSE_FOLLOWUP_PROMPT.contains("不要继续调用工具"));
    assert!(EMPTY_FINAL_RESPONSE_FOLLOWUP_PROMPT.contains("继续使用必要工具"));
    assert!(EMPTY_FINAL_RESPONSE_FOLLOWUP_PROMPT.contains("不要把未完成工作包装成最终结果"));
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
