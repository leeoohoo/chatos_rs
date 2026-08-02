// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::post,
    Json, Router,
};
use serde_json::{json, Value};
use tokio::sync::Mutex as AsyncMutex;

use chatos_mcp_runtime::{ToolCallContext, ToolCallerModelRuntime, ToolResult, ToolResultCallback};

use super::{
    append_runtime_input_items, empty_final_response_followup_item,
    merge_current_turn_tool_history_into_input, merge_pending_tool_turn_into_input,
    merge_record_metadata, prepare_iteration_request, should_persist_tool_result,
    IterativeContextRefresh, EMPTY_FINAL_RESPONSE_FOLLOWUP_PROMPT,
};
use crate::{
    AiResponse, AiRuntime, AiRuntimeOptions, AiRuntimeResult, AiTurnReport, AiTurnStatus,
    ModelRequest, RuntimeBeforeModelRequest, RuntimeCallbacks, RuntimeFinalResponseAction,
    RuntimeFinalResponseContext, RuntimeIterationContext, RuntimeLifecycleHook, ToolExecutor,
};

struct TestLifecycleHook;

struct PagingToolExecutor;

#[async_trait]
impl ToolExecutor for PagingToolExecutor {
    fn available_tools(&self) -> Vec<Value> {
        vec![json!({
            "type": "function",
            "name": "list_page",
            "description": "List one page",
            "parameters": {
                "type": "object",
                "properties": {"offset": {"type": "integer"}}
            }
        })]
    }

    async fn execute_tools_stream(
        &self,
        tool_calls: &[Value],
        _context: ToolCallContext,
        _on_tool_result: Option<ToolResultCallback>,
    ) -> Vec<ToolResult> {
        tool_calls
            .iter()
            .map(|call| {
                let call_id = call
                    .get("call_id")
                    .or_else(|| call.get("id"))
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                ToolResult {
                    tool_call_id: call_id.clone(),
                    name: "list_page".to_string(),
                    success: true,
                    is_error: false,
                    is_stream: false,
                    conversation_turn_id: None,
                    content: format!("result-{call_id}"),
                    result: None,
                    fatal_error: false,
                    transient_model_input: None,
                }
            })
            .collect()
    }
}

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

struct FilterToolsLifecycleHook;

#[async_trait]
impl RuntimeLifecycleHook for FilterToolsLifecycleHook {
    async fn before_model_request(
        &self,
        _context: RuntimeIterationContext,
    ) -> Result<RuntimeBeforeModelRequest, String> {
        Ok(RuntimeBeforeModelRequest::unchanged()
            .with_disabled_tool_names(["read_file", "list_tasks"]))
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

#[tokio::test]
async fn lifecycle_hook_can_disable_selected_tools_for_one_iteration() {
    let request = ModelRequest::openai_compatible(
        "http://localhost",
        "key",
        "model",
        "openai_compatible",
        json!([{"role": "user", "content": "hello"}]),
    )
    .with_tools(vec![
        json!({"name": "read_file"}),
        json!({"type": "function", "function": {"name": "list_tasks"}}),
        json!({"name": "write_file"}),
    ]);
    let options = AiRuntimeOptions::for_conversation("session-1")
        .with_lifecycle_hook(Some(Arc::new(FilterToolsLifecycleHook)));

    let (iteration_request, directive) =
        prepare_iteration_request(&request, &options, 1, "initial")
            .await
            .expect("iteration request");

    assert_eq!(iteration_request.tools, vec![json!({"name": "write_file"})]);
    assert_eq!(directive.disabled_tool_names, ["read_file", "list_tasks"]);
    assert_eq!(request.tools.len(), 3);
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
    connection_headers: Arc<AsyncMutex<Vec<Option<String>>>>,
}

async fn mock_lifecycle_provider(
    State(state): State<MockLifecycleProviderState>,
    headers: HeaderMap,
    Json(payload): Json<Value>,
) -> (StatusCode, Json<Value>) {
    state.requests.lock().await.push(payload);
    state.connection_headers.lock().await.push(
        headers
            .get(reqwest::header::CONNECTION)
            .and_then(|value| value.to_str().ok())
            .map(ToOwned::to_owned),
    );
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
    Arc<AsyncMutex<Vec<Option<String>>>>,
    tokio::task::JoinHandle<()>,
) {
    let connection_headers = Arc::new(AsyncMutex::new(Vec::new()));
    let state = MockLifecycleProviderState {
        responses: Arc::new(AsyncMutex::new(responses.into_iter().collect())),
        requests: Arc::new(AsyncMutex::new(Vec::new())),
        connection_headers: Arc::clone(&connection_headers),
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
    (
        format!("http://{address}"),
        requests,
        connection_headers,
        server,
    )
}

#[tokio::test]
async fn iterative_context_refresh_keeps_prior_tool_batches_in_later_model_requests() {
    let (base_url, requests, _connection_headers, server) = start_lifecycle_mock_provider(vec![
        json!({
            "id": "response-page-1",
            "status": "completed",
            "output": [{
                "type": "function_call",
                "id": "fc_1",
                "call_id": "call_1",
                "name": "list_page",
                "arguments": "{\"offset\":0}",
                "status": "completed"
            }]
        }),
        json!({
            "id": "response-page-2",
            "status": "completed",
            "output": [{
                "type": "function_call",
                "id": "fc_2",
                "call_id": "call_2",
                "name": "list_page",
                "arguments": "{\"offset\":4}",
                "status": "completed"
            }]
        }),
        json!({
            "id": "response-final",
            "status": "completed",
            "output_text": "verified both pages"
        }),
    ])
    .await;
    let request = ModelRequest::openai_compatible(
        base_url,
        "test-key",
        "gpt-test",
        "openai",
        json!([{"role": "user", "content": "verify every page"}]),
    )
    .with_responses_support(true);
    let refresh =
        IterativeContextRefresh::new(None, None, Vec::new()).with_sticky_input_items(vec![
            json!({"role": "user", "content": "verify every page"}),
        ]);
    let options = AiRuntimeOptions::for_conversation("session-current-turn-history")
        .with_iterative_context_refresh(Some(refresh));

    let result = AiRuntime::new(Some(Arc::new(PagingToolExecutor)))
        .with_max_iterations(4)
        .run_turn(request, options)
        .await
        .expect("paginated tool turn");
    server.abort();

    assert_eq!(result.content, "verified both pages");
    let requests = requests.lock().await;
    assert_eq!(requests.len(), 3);
    assert!(requests[1].to_string().contains("result-call_1"));
    assert!(requests[2].to_string().contains("result-call_1"));
    assert!(requests[2].to_string().contains("result-call_2"));
}

#[derive(Clone, Default)]
struct ParseRecoveryProviderState {
    requests: Arc<AsyncMutex<Vec<Value>>>,
    connection_headers: Arc<AsyncMutex<Vec<Option<String>>>>,
    accept_encoding_headers: Arc<AsyncMutex<Vec<Option<String>>>>,
    always_fail: bool,
}

async fn mock_parse_recovery_provider(
    State(state): State<ParseRecoveryProviderState>,
    headers: HeaderMap,
    Json(payload): Json<Value>,
) -> Response {
    let mut requests = state.requests.lock().await;
    requests.push(payload);
    let request_count = requests.len();
    drop(requests);
    state.connection_headers.lock().await.push(
        headers
            .get(reqwest::header::CONNECTION)
            .and_then(|value| value.to_str().ok())
            .map(ToOwned::to_owned),
    );
    state.accept_encoding_headers.lock().await.push(
        headers
            .get(reqwest::header::ACCEPT_ENCODING)
            .and_then(|value| value.to_str().ok())
            .map(ToOwned::to_owned),
    );

    if request_count == 1 || state.always_fail {
        return (
            StatusCode::OK,
            [(reqwest::header::CONTENT_TYPE, "application/json")],
            "{truncated json",
        )
            .into_response();
    }

    Json(json!({
        "id": "response-recovered",
        "status": "completed",
        "output_text": "completed through non-stream fallback",
        "output": []
    }))
    .into_response()
}

async fn start_parse_recovery_mock_provider(
    always_fail: bool,
) -> (
    String,
    ParseRecoveryProviderState,
    tokio::task::JoinHandle<()>,
) {
    let state = ParseRecoveryProviderState {
        always_fail,
        ..ParseRecoveryProviderState::default()
    };
    let app = Router::new()
        .route("/responses", post(mock_parse_recovery_provider))
        .with_state(state.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind parse recovery mock provider");
    let address = listener.local_addr().expect("mock provider address");
    let server = tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    (format!("http://{address}"), state, server)
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
    let (base_url, requests, _connection_headers, server) = start_lifecycle_mock_provider(vec![
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
    let (base_url, requests, connection_headers, server) = start_lifecycle_mock_provider(vec![
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
    let connection_headers = connection_headers.lock().await;
    assert_eq!(connection_headers.first(), Some(&None));
    assert!(connection_headers
        .iter()
        .skip(1)
        .all(|header| header.as_deref() == Some("close")));
}

#[tokio::test]
async fn model_request_uses_configured_transient_retry_limit() {
    let failed_response = json!({
        "id": "response-failed",
        "status": "failed",
        "error": null
    });
    let (base_url, requests, _connection_headers, server) = start_lifecycle_mock_provider(vec![
        failed_response.clone(),
        failed_response.clone(),
        failed_response,
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
    .with_max_transient_retries(Some(2));

    let error = AiRuntime::new(None)
        .with_max_iterations(2)
        .run_turn(request, AiRuntimeOptions::for_conversation("session-retry"))
        .await
        .expect_err("configured retries should be exhausted");
    server.abort();

    assert!(error.contains("已重试 2 次"));
    assert_eq!(requests.lock().await.len(), 3);
}

#[tokio::test]
async fn stream_parse_failure_retries_once_in_non_stream_isolated_mode() {
    let (base_url, state, server) = start_parse_recovery_mock_provider(false).await;
    let diagnostics = Arc::new(Mutex::new(Vec::new()));
    let request = ModelRequest::openai_compatible(
        base_url,
        "test-key",
        "gpt-test",
        "openai",
        json!([{"role": "user", "content": "complete the task"}]),
    )
    .with_responses_support(true)
    .with_max_transient_retries(Some(1));

    let result = AiRuntime::new(None)
        .with_max_iterations(2)
        .run_turn(
            request,
            AiRuntimeOptions::for_conversation("session-parse-recovery").with_callbacks(
                RuntimeCallbacks {
                    on_before_model_request: Some(Arc::new({
                        let diagnostics = Arc::clone(&diagnostics);
                        move |payload| diagnostics.lock().expect("diagnostics").push(payload)
                    })),
                    ..RuntimeCallbacks::default()
                },
            ),
        )
        .await
        .expect("non-stream recovery should succeed");
    server.abort();

    assert_eq!(result.content, "completed through non-stream fallback");
    let requests = state.requests.lock().await;
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[0].get("stream"), Some(&Value::Bool(true)));
    assert_eq!(requests[1].get("stream"), Some(&Value::Bool(false)));
    drop(requests);

    let connection_headers = state.connection_headers.lock().await;
    assert_eq!(connection_headers[0], None);
    assert_eq!(connection_headers[1].as_deref(), Some("close"));
    drop(connection_headers);

    let accept_encoding_headers = state.accept_encoding_headers.lock().await;
    assert_eq!(accept_encoding_headers[1].as_deref(), Some("identity"));

    let diagnostics = diagnostics.lock().expect("diagnostics");
    assert_eq!(diagnostics.len(), 2);
    assert_eq!(diagnostics[0]["task_runner_debug"]["request_attempt"], 1);
    assert_eq!(diagnostics[0]["task_runner_debug"]["stream"], true);
    assert_eq!(diagnostics[1]["task_runner_debug"]["request_attempt"], 2);
    assert_eq!(diagnostics[1]["task_runner_debug"]["stream"], false);
    assert_eq!(
        diagnostics[1]["task_runner_debug"]["connection_mode"],
        "isolated_retry"
    );
}

#[tokio::test]
async fn exhausted_parse_recovery_reports_non_stream_fallback_without_raw_body() {
    let (base_url, state, server) = start_parse_recovery_mock_provider(true).await;
    let request = ModelRequest::openai_compatible(
        base_url,
        "test-key",
        "gpt-test",
        "openai",
        json!([{"role": "user", "content": "complete the task"}]),
    )
    .with_responses_support(true)
    .with_max_transient_retries(Some(1));

    let error = AiRuntime::new(None)
        .with_max_iterations(2)
        .run_turn(
            request,
            AiRuntimeOptions::for_conversation("session-parse-exhausted"),
        )
        .await
        .expect_err("two malformed responses should exhaust recovery");
    server.abort();

    assert_eq!(state.requests.lock().await.len(), 2);
    assert!(error.contains("最后一次响应包含无法解析的数据"));
    assert!(error.contains("已自动切换为非流式响应"));
    assert!(!error.contains("truncated json"));
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
fn refreshed_context_keeps_every_tool_batch_from_the_current_turn() {
    let refreshed = json!([
        {"type":"message","role":"user","content":[]},
        {"type":"function_call","call_id":"call_2","name":"list_tasks","arguments":"{\"offset\":4}"},
        {"type":"function_call_output","call_id":"call_2","output":"page-2-from-memory"}
    ]);
    let calls = vec![
        json!({"type":"function_call","call_id":"call_1","name":"list_tasks","arguments":"{\"offset\":0}"}),
        json!({"type":"function_call","call_id":"call_2","name":"list_tasks","arguments":"{\"offset\":4}"}),
    ];
    let outputs = vec![
        json!({"type":"function_call_output","call_id":"call_1","output":"page-1"}),
        json!({"type":"function_call_output","call_id":"call_2","output":"page-2-authoritative"}),
    ];

    let merged = merge_current_turn_tool_history_into_input(
        refreshed,
        calls.as_slice(),
        outputs.as_slice(),
        None,
    );
    let items = merged.as_array().expect("items");

    assert_eq!(
        items
            .iter()
            .filter(|item| { item.get("type").and_then(Value::as_str) == Some("function_call") })
            .count(),
        2
    );
    assert!(items.iter().any(|item| {
        item.get("call_id").and_then(Value::as_str) == Some("call_1")
            && item.get("output").and_then(Value::as_str) == Some("page-1")
    }));
    assert!(items.iter().any(|item| {
        item.get("call_id").and_then(Value::as_str) == Some("call_2")
            && item.get("output").and_then(Value::as_str) == Some("page-2-authoritative")
    }));
}

#[test]
fn current_turn_tool_history_budget_prefers_newest_results_without_forgetting_old_calls() {
    let calls = vec![
        json!({"type":"function_call","call_id":"call_old","name":"read_page","arguments":"{\"offset\":0}"}),
        json!({"type":"function_call","call_id":"call_new","name":"read_page","arguments":"{\"offset\":4}"}),
    ];
    let outputs = vec![
        json!({"type":"function_call_output","call_id":"call_old","output":"older-page"}),
        json!({"type":"function_call_output","call_id":"call_new","output":"latest-page"}),
    ];

    let merged = merge_current_turn_tool_history_into_input(
        json!([]),
        calls.as_slice(),
        outputs.as_slice(),
        Some(crate::tool_runtime::ToolResultModelBudgetLimits::new(
            100, 11,
        )),
    );
    let items = merged.as_array().expect("items");
    let old_output = items
        .iter()
        .find(|item| {
            item.get("type").and_then(Value::as_str) == Some("function_call_output")
                && item.get("call_id").and_then(Value::as_str) == Some("call_old")
        })
        .and_then(|item| item.get("output"))
        .and_then(Value::as_str)
        .expect("old output");
    let new_output = items
        .iter()
        .find(|item| {
            item.get("type").and_then(Value::as_str) == Some("function_call_output")
                && item.get("call_id").and_then(Value::as_str) == Some("call_new")
        })
        .and_then(|item| item.get("output"))
        .and_then(Value::as_str)
        .expect("new output");

    assert!(old_output.contains("combined tool results exceed"));
    assert_eq!(new_output, "latest-page");
    assert_eq!(
        items
            .iter()
            .filter(|item| { item.get("type").and_then(Value::as_str) == Some("function_call") })
            .count(),
        2
    );
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
fn should_persist_every_completed_tool_result_including_empty_arrays() {
    let empty_success = tool_result("[]", Some(json!([])), true, false, false);
    assert!(should_persist_tool_result(&empty_success));

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
        fatal_error: false,
        transient_model_input: None,
    }
}
