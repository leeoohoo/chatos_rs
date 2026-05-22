use axum::http::StatusCode;
use serde_json::{json, Value};

use super::test_support::{
    before_request_set_task_done_on_nth_request, build_test_client,
    build_test_client_with_max_iterations, chunk_callbacks, demo_echo_tool, empty_callbacks,
    run_process_with_tools, setup_sqlite_task_board, start_mock_provider, MockProviderStep,
    RunProcessWithToolsArgs,
};
use crate::services::task_manager::TaskDraft;

#[tokio::test]
async fn completion_overflow_without_remote_summary_surfaces_error() {
    let steps = vec![
        MockProviderStep::text(
            StatusCode::BAD_REQUEST,
            "unsupported parameter: previous_response_id",
        ),
        MockProviderStep::json(
            StatusCode::OK,
            json!({
                "id": "resp_failed",
                "status": "failed",
                "error": { "message": "context_length_exceeded: input exceeds the context window" }
            }),
        ),
    ];
    let (base_url, captured, server) = start_mock_provider(steps).await;
    let mut client = build_test_client(base_url);

    let err = run_process_with_tools(
        &mut client,
        RunProcessWithToolsArgs {
            previous_response_id: Some("prev_resp_1".to_string()),
            callbacks: empty_callbacks(),
            use_prev_id: true,
            can_use_prev_id: true,
            ..Default::default()
        },
    )
    .await
    .expect_err("without remote summary support, overflow should surface");
    server.abort();

    assert!(err.contains("context_length_exceeded"), "{err}");

    let requests = captured.lock().await.clone();
    assert_eq!(requests.len(), 2);
    assert!(requests[0].get("previous_response_id").is_some());
    assert!(requests[1].get("previous_response_id").is_none());
    assert!(requests[1]
        .get("input")
        .map(|value| value.is_array())
        .unwrap_or(false));
}

#[tokio::test]
async fn stable_prefixed_items_keep_previous_response_id_reuse() {
    let steps = vec![MockProviderStep::json(
        StatusCode::OK,
        json!({
            "id": "resp_contact_stable",
            "status": "completed",
            "output_text": "contact stable ok"
        }),
    )];
    let (base_url, captured, server) = start_mock_provider(steps).await;
    let mut client = build_test_client(base_url);

    let result = run_process_with_tools(
        &mut client,
        RunProcessWithToolsArgs {
            session_id: Some("session_contact_stable".to_string()),
            previous_response_id: Some("prev_resp_contact_stable".to_string()),
            prompt_cache_key: Some("session_contact_stable".to_string()),
            callbacks: empty_callbacks(),
            use_prev_id: true,
            can_use_prev_id: true,
            prefixed_input_items: vec![json!({
                "type": "message",
                "role": "system",
                "content": [
                    {
                        "type": "input_text",
                        "text": "[Task Board]\n当前任务看板由系统维护"
                    }
                ]
            })],
            stable_prefix_mode: true,
            purpose: "chat",
            ..Default::default()
        },
    )
    .await
    .expect("stable prefixed items should preserve stateful request");
    server.abort();

    assert_eq!(
        result.get("content").and_then(|value| value.as_str()),
        Some("contact stable ok")
    );

    let requests = captured.lock().await.clone();
    assert_eq!(requests.len(), 1);
    assert_eq!(
        requests[0]
            .get("previous_response_id")
            .and_then(|value| value.as_str()),
        Some("prev_resp_contact_stable")
    );
    assert_eq!(
        requests[0]
            .get("prompt_cache_key")
            .and_then(|value| value.as_str()),
        Some("session_contact_stable")
    );
}

#[tokio::test]
async fn task_follow_up_continues_same_turn_until_unfinished_tasks_finish() {
    let session_id = "session_task_follow_up_continue";
    let turn_id = "turn_task_follow_up_continue";
    let tasks = vec![TaskDraft {
        title: "First unfinished task".to_string(),
        details: "keep working".to_string(),
        priority: "medium".to_string(),
        status: "doing".to_string(),
        tags: vec![],
        due_at: None,
        outcome_summary: String::new(),
        outcome_items: vec![],
        resume_hint: String::new(),
        blocker_reason: String::new(),
        blocker_needs: vec![],
        blocker_kind: String::new(),
    }];
    let created = setup_sqlite_task_board(session_id, turn_id, tasks)
        .await
        .expect("setup board");
    let task_id = created[0].id.clone();
    let steps = vec![
        MockProviderStep::json(
            StatusCode::OK,
            json!({
                "id": "resp_summary_1",
                "status": "completed",
                "output_text": "summary"
            }),
        ),
        MockProviderStep::json(
            StatusCode::OK,
            json!({
                "id": "resp_continue_2",
                "status": "completed",
                "output_text": "continue work"
            }),
        ),
        MockProviderStep::json(
            StatusCode::OK,
            json!({
                "id": "resp_review_pass_3",
                "status": "completed",
                "output_text": "TASK_REVIEW: pass\nall good"
            }),
        ),
    ];
    let (base_url, captured, server) = start_mock_provider(steps).await;
    let mut client = build_test_client_with_max_iterations(base_url, 4);
    let callbacks = before_request_set_task_done_on_nth_request(session_id.to_string(), task_id, 2);

    let result = run_process_with_tools(
        &mut client,
        RunProcessWithToolsArgs {
            session_id: Some(session_id.to_string()),
            turn_id: Some(turn_id.to_string()),
            callbacks,
            purpose: "chat",
            use_prev_id: true,
            can_use_prev_id: true,
            ..Default::default()
        },
    )
    .await
    .expect("follow-up should continue and finish");
    server.abort();

    assert_eq!(
        result.get("content").and_then(|value| value.as_str()),
        Some("continue work")
    );

    let requests = captured.lock().await.clone();
    assert_eq!(requests.len(), 3);
    assert!(requests[0]
        .get("previous_response_id")
        .and_then(|value| value.as_str())
        .is_none());
    assert_eq!(
        requests[1]
            .get("previous_response_id")
            .and_then(|value| value.as_str()),
        Some("resp_summary_1")
    );
    assert_eq!(
        requests[2]
            .get("previous_response_id")
            .and_then(|value| value.as_str()),
        Some("resp_continue_2")
    );
}

#[tokio::test]
async fn task_follow_up_reviews_same_turn_when_work_is_done() {
    let session_id = "session_task_follow_up_review";
    let turn_id = "turn_task_follow_up_review";
    let tasks = vec![TaskDraft {
        title: "Finished task".to_string(),
        details: "already done".to_string(),
        priority: "medium".to_string(),
        status: "done".to_string(),
        tags: vec![],
        due_at: None,
        outcome_summary: String::new(),
        outcome_items: vec![],
        resume_hint: String::new(),
        blocker_reason: String::new(),
        blocker_needs: vec![],
        blocker_kind: String::new(),
    }];
    setup_sqlite_task_board(session_id, turn_id, tasks)
        .await
        .expect("setup board");
    let steps = vec![
        MockProviderStep::json(
            StatusCode::OK,
            json!({
                "id": "resp_summary_review",
                "status": "completed",
                "output_text": "summary"
            }),
        ),
        MockProviderStep::json(
            StatusCode::OK,
            json!({
                "id": "resp_review_pass",
                "status": "completed",
                "output_text": "TASK_REVIEW: pass\nlooks good"
            }),
        ),
    ];
    let (base_url, captured, server) = start_mock_provider(steps).await;
    let mut client = build_test_client_with_max_iterations(base_url, 4);

    let result = run_process_with_tools(
        &mut client,
        RunProcessWithToolsArgs {
            session_id: Some(session_id.to_string()),
            turn_id: Some(turn_id.to_string()),
            callbacks: empty_callbacks(),
            purpose: "chat",
            use_prev_id: true,
            can_use_prev_id: true,
            ..Default::default()
        },
    )
    .await
    .expect("follow-up should review and finish");
    server.abort();

    assert_eq!(
        result.get("content").and_then(|value| value.as_str()),
        Some("summary")
    );

    let requests = captured.lock().await.clone();
    assert_eq!(requests.len(), 2);
    assert_eq!(
        requests[1]
            .get("previous_response_id")
            .and_then(|value| value.as_str()),
        Some("resp_summary_review")
    );
}

#[tokio::test]
async fn runtime_guidance_items_disable_previous_response_id_reuse() {
    let steps = vec![MockProviderStep::json(
        StatusCode::OK,
        json!({
            "id": "resp_contact_runtime",
            "status": "completed",
            "output_text": "contact runtime ok"
        }),
    )];
    let (base_url, captured, server) = start_mock_provider(steps).await;
    let mut client = build_test_client(base_url);

    let result = run_process_with_tools(
        &mut client,
        RunProcessWithToolsArgs {
            session_id: Some("session_contact_runtime".to_string()),
            previous_response_id: Some("prev_resp_contact_runtime".to_string()),
            callbacks: empty_callbacks(),
            use_prev_id: true,
            can_use_prev_id: true,
            prefixed_input_items: vec![json!({
                "type": "message",
                "role": "system",
                "content": [
                    {
                        "type": "input_text",
                        "text": "[Runtime Guidance]\n- instruction: 联系人 runtime context"
                    }
                ]
            })],
            stable_prefix_mode: false,
            purpose: "chat",
            ..Default::default()
        },
    )
    .await
    .expect("prefixed runtime items should force stateless request");
    server.abort();

    assert_eq!(
        result.get("content").and_then(|value| value.as_str()),
        Some("contact runtime ok")
    );

    let requests = captured.lock().await.clone();
    assert_eq!(requests.len(), 1);
    assert!(requests[0].get("previous_response_id").is_none());
}

#[tokio::test]
async fn recovers_input_must_be_list_and_retries_with_list_payload() {
    let steps = vec![
        MockProviderStep::text(StatusCode::BAD_REQUEST, "input must be a list"),
        MockProviderStep::json(
            StatusCode::OK,
            json!({
                "id": "resp_ok",
                "status": "completed",
                "output_text": "list retry success"
            }),
        ),
    ];
    let (base_url, captured, server) = start_mock_provider(steps).await;
    let mut client = build_test_client(base_url);

    let result = run_process_with_tools(
        &mut client,
        RunProcessWithToolsArgs {
            callbacks: empty_callbacks(),
            ..Default::default()
        },
    )
    .await
    .expect("process should recover input list constraint");
    server.abort();

    assert_eq!(
        result.get("content").and_then(|value| value.as_str()),
        Some("list retry success")
    );

    let requests = captured.lock().await.clone();
    assert_eq!(requests.len(), 2);
    assert!(requests[0]
        .get("input")
        .map(|value| value.is_string())
        .unwrap_or(false));
    assert!(requests[1]
        .get("input")
        .map(|value| value.is_array())
        .unwrap_or(false));
}

#[tokio::test]
async fn retries_completion_failure_when_provider_is_overloaded() {
    let steps = vec![
        MockProviderStep::json(
            StatusCode::OK,
            json!({
                "id": "resp_failed_overloaded",
                "status": "failed",
                "error": {
                    "code": "server_is_overloaded",
                    "message": "Our servers are currently overloaded. Please try again later."
                }
            }),
        ),
        MockProviderStep::json(
            StatusCode::OK,
            json!({
                "id": "resp_ok_after_retry",
                "status": "completed",
                "output_text": "retry after overload success"
            }),
        ),
    ];
    let (base_url, captured, server) = start_mock_provider(steps).await;
    let mut client = build_test_client(base_url);

    let result = run_process_with_tools(
        &mut client,
        RunProcessWithToolsArgs {
            callbacks: empty_callbacks(),
            ..Default::default()
        },
    )
    .await
    .expect("completion overload should retry");
    server.abort();

    assert_eq!(
        result.get("content").and_then(|value| value.as_str()),
        Some("retry after overload success")
    );

    let requests = captured.lock().await.clone();
    assert_eq!(requests.len(), 2);
}

#[tokio::test]
async fn retries_completion_failure_when_model_is_at_capacity() {
    let steps = vec![
        MockProviderStep::json(
            StatusCode::OK,
            json!({
                "id": "resp_failed_capacity",
                "status": "failed",
                "error": {
                    "message": "Selected model is at capacity. Please try a different model."
                }
            }),
        ),
        MockProviderStep::json(
            StatusCode::OK,
            json!({
                "id": "resp_ok_after_capacity_retry",
                "status": "completed",
                "output_text": "retry after capacity success"
            }),
        ),
    ];
    let (base_url, captured, server) = start_mock_provider(steps).await;
    let mut client = build_test_client(base_url);

    let result = run_process_with_tools(
        &mut client,
        RunProcessWithToolsArgs {
            callbacks: empty_callbacks(),
            ..Default::default()
        },
    )
    .await
    .expect("completion capacity should retry");
    server.abort();

    assert_eq!(
        result.get("content").and_then(|value| value.as_str()),
        Some("retry after capacity success")
    );

    let requests = captured.lock().await.clone();
    assert_eq!(requests.len(), 2);
}

#[tokio::test]
async fn tool_follow_up_reuses_previous_response_id_with_incremental_outputs() {
    let steps = vec![
        MockProviderStep::json(
            StatusCode::OK,
            json!({
                "id": "resp_tool_1",
                "status": "completed",
                "output": [{
                    "type": "function_call",
                    "call_id": "call_tool_1",
                    "name": "demo_echo",
                    "arguments": "{\"text\":\"hello\"}"
                }]
            }),
        ),
        MockProviderStep::json(
            StatusCode::OK,
            json!({
                "id": "resp_tool_done",
                "status": "completed",
                "output_text": "tool recovery success"
            }),
        ),
    ];
    let (base_url, captured, server) = start_mock_provider(steps).await;
    let mut client = build_test_client(base_url);

    let result = run_process_with_tools(
        &mut client,
        RunProcessWithToolsArgs {
            previous_response_id: Some("prev_resp_seed".to_string()),
            tools: vec![demo_echo_tool()],
            callbacks: empty_callbacks(),
            use_prev_id: true,
            can_use_prev_id: true,
            ..Default::default()
        },
    )
    .await
    .expect("tool follow-up should reuse previous_response_id");
    server.abort();

    assert_eq!(
        result.get("content").and_then(|value| value.as_str()),
        Some("tool recovery success")
    );

    let requests = captured.lock().await.clone();
    assert_eq!(requests.len(), 2);

    assert_eq!(
        requests[0]
            .get("previous_response_id")
            .and_then(|value| value.as_str()),
        Some("prev_resp_seed")
    );
    assert_eq!(
        requests[1]
            .get("previous_response_id")
            .and_then(|value| value.as_str()),
        Some("resp_tool_1")
    );
    assert!(requests[1]
        .get("input")
        .and_then(|value| value.as_array())
        .map(|items| {
            let has_output = items.iter().any(|item| {
                item.get("type").and_then(|value| value.as_str()) == Some("function_call_output")
                    && item.get("call_id").and_then(|value| value.as_str()) == Some("call_tool_1")
            });
            items.len() == 1 && has_output
        })
        .unwrap_or(false));
}

#[tokio::test]
async fn falls_back_to_stateless_when_tool_call_response_has_no_response_id() {
    let steps = vec![
        MockProviderStep::json(
            StatusCode::OK,
            json!({
                "status": "completed",
                "output": [{
                    "type": "function_call",
                    "call_id": "call_tool_missing_resp_id",
                    "name": "demo_echo",
                    "arguments": "{\"text\":\"hello\"}"
                }]
            }),
        ),
        MockProviderStep::json(
            StatusCode::OK,
            json!({
                "id": "resp_after_stateless_fallback",
                "status": "completed",
                "output_text": "stateless fallback success"
            }),
        ),
    ];
    let (base_url, captured, server) = start_mock_provider(steps).await;
    let mut client = build_test_client(base_url);

    let result = run_process_with_tools(
        &mut client,
        RunProcessWithToolsArgs {
            previous_response_id: Some("prev_resp_tool_seed".to_string()),
            tools: vec![demo_echo_tool()],
            callbacks: empty_callbacks(),
            use_prev_id: true,
            can_use_prev_id: true,
            ..Default::default()
        },
    )
    .await
    .expect("missing tool response_id should fallback to stateless mode");
    server.abort();

    assert_eq!(
        result.get("content").and_then(|value| value.as_str()),
        Some("stateless fallback success")
    );

    let requests = captured.lock().await.clone();
    assert_eq!(requests.len(), 2);
    assert_eq!(
        requests[0]
            .get("previous_response_id")
            .and_then(|value| value.as_str()),
        Some("prev_resp_tool_seed")
    );
    assert!(requests[1].get("previous_response_id").is_none());
    assert!(requests[1]
        .get("input")
        .and_then(|value| value.as_array())
        .map(|items| {
            let has_user = items
                .iter()
                .any(|item| item.get("role").and_then(|value| value.as_str()) == Some("user"));
            let has_call = items.iter().any(|item| {
                item.get("type").and_then(|value| value.as_str()) == Some("function_call")
                    && item.get("call_id").and_then(|value| value.as_str())
                        == Some("call_tool_missing_resp_id")
            });
            let has_output = items.iter().any(|item| {
                item.get("type").and_then(|value| value.as_str()) == Some("function_call_output")
                    && item.get("call_id").and_then(|value| value.as_str())
                        == Some("call_tool_missing_resp_id")
            });
            has_user && has_call && has_output
        })
        .unwrap_or(false));
}

#[tokio::test]
async fn falls_back_to_stateless_when_incremental_tool_outputs_are_rejected() {
    let steps = vec![
        MockProviderStep::json(
            StatusCode::OK,
            json!({
                "id": "resp_tool_prev_id_seed",
                "status": "completed",
                "output": [{
                    "type": "function_call",
                    "call_id": "call_tool_prev_id_seed",
                    "name": "demo_echo",
                    "arguments": "{\"text\":\"hello\"}"
                }]
            }),
        ),
        MockProviderStep::text(
            StatusCode::BAD_REQUEST,
            "No tool call found for function_call_output item",
        ),
        MockProviderStep::json(
            StatusCode::OK,
            json!({
                "id": "resp_tool_prev_id_recovered",
                "status": "completed",
                "output_text": "tool prev-id fallback success"
            }),
        ),
    ];
    let (base_url, captured, server) = start_mock_provider(steps).await;
    let mut client = build_test_client(base_url);

    let result = run_process_with_tools(
        &mut client,
        RunProcessWithToolsArgs {
            previous_response_id: Some("prev_resp_tool_prev_id_seed".to_string()),
            tools: vec![demo_echo_tool()],
            callbacks: empty_callbacks(),
            use_prev_id: true,
            can_use_prev_id: true,
            ..Default::default()
        },
    )
    .await
    .expect("rejected incremental tool outputs should fallback to stateless mode");
    server.abort();

    assert_eq!(
        result.get("content").and_then(|value| value.as_str()),
        Some("tool prev-id fallback success")
    );

    let requests = captured.lock().await.clone();
    assert_eq!(requests.len(), 3);
    assert_eq!(
        requests[1]
            .get("previous_response_id")
            .and_then(|value| value.as_str()),
        Some("resp_tool_prev_id_seed")
    );
    assert!(requests[2].get("previous_response_id").is_none());
    assert!(requests[2]
        .get("input")
        .and_then(|value| value.as_array())
        .map(|items| {
            let has_user = items
                .iter()
                .any(|item| item.get("role").and_then(|value| value.as_str()) == Some("user"));
            let has_call = items.iter().any(|item| {
                item.get("type").and_then(|value| value.as_str()) == Some("function_call")
                    && item.get("call_id").and_then(|value| value.as_str())
                        == Some("call_tool_prev_id_seed")
            });
            let has_output = items.iter().any(|item| {
                item.get("type").and_then(|value| value.as_str()) == Some("function_call_output")
                    && item.get("call_id").and_then(|value| value.as_str())
                        == Some("call_tool_prev_id_seed")
            });
            has_user && has_call && has_output
        })
        .unwrap_or(false));
}

#[tokio::test]
async fn reenters_previous_response_id_after_missing_tool_call_fallback() {
    let steps = vec![
        MockProviderStep::json(
            StatusCode::OK,
            json!({
                "id": "resp_tool_prev_id_seed",
                "status": "completed",
                "output": [{
                    "type": "function_call",
                    "call_id": "call_tool_prev_id_seed",
                    "name": "demo_echo",
                    "arguments": "{\"text\":\"hello\"}"
                }]
            }),
        ),
        MockProviderStep::text(
            StatusCode::BAD_REQUEST,
            "No tool call found for function_call_output item",
        ),
        MockProviderStep::json(
            StatusCode::OK,
            json!({
                "id": "resp_tool_prev_id_recovered",
                "status": "completed",
                "output": [{
                    "type": "function_call",
                    "call_id": "call_tool_prev_id_recovered",
                    "name": "demo_echo",
                    "arguments": "{\"text\":\"world\"}"
                }]
            }),
        ),
        MockProviderStep::json(
            StatusCode::OK,
            json!({
                "id": "resp_tool_prev_id_final",
                "status": "completed",
                "output_text": "tool prev-id resumed"
            }),
        ),
    ];
    let (base_url, captured, server) = start_mock_provider(steps).await;
    let mut client = build_test_client(base_url);

    let result = run_process_with_tools(
        &mut client,
        RunProcessWithToolsArgs {
            previous_response_id: Some("prev_resp_tool_prev_id_seed".to_string()),
            tools: vec![demo_echo_tool()],
            callbacks: empty_callbacks(),
            use_prev_id: true,
            can_use_prev_id: true,
            ..Default::default()
        },
    )
    .await
    .expect("missing tool-call fallback should recover previous_response_id on later tool rounds");
    server.abort();

    assert_eq!(
        result.get("content").and_then(|value| value.as_str()),
        Some("tool prev-id resumed")
    );

    let requests = captured.lock().await.clone();
    assert_eq!(requests.len(), 4);
    assert_eq!(
        requests[1]
            .get("previous_response_id")
            .and_then(|value| value.as_str()),
        Some("resp_tool_prev_id_seed")
    );
    assert!(requests[2].get("previous_response_id").is_none());
    assert!(requests[2]
        .get("input")
        .and_then(|value| value.as_array())
        .map(|items| {
            let has_call = items.iter().any(|item| {
                item.get("type").and_then(|value| value.as_str()) == Some("function_call")
                    && item.get("call_id").and_then(|value| value.as_str())
                        == Some("call_tool_prev_id_seed")
            });
            let has_output = items.iter().any(|item| {
                item.get("type").and_then(|value| value.as_str()) == Some("function_call_output")
                    && item.get("call_id").and_then(|value| value.as_str())
                        == Some("call_tool_prev_id_seed")
            });
            has_call && has_output
        })
        .unwrap_or(false));
    assert_eq!(
        requests[3]
            .get("previous_response_id")
            .and_then(|value| value.as_str()),
        Some("resp_tool_prev_id_recovered")
    );
    assert!(requests[3]
        .get("input")
        .and_then(|value| value.as_array())
        .map(|items| {
            let has_output = items.iter().any(|item| {
                item.get("type").and_then(|value| value.as_str()) == Some("function_call_output")
                    && item.get("call_id").and_then(|value| value.as_str())
                        == Some("call_tool_prev_id_recovered")
            });
            items.len() == 1 && has_output
        })
        .unwrap_or(false));
}

#[tokio::test]
async fn recovers_missing_tool_call_output_in_stream_mode_with_pending_items_merged() {
    let first_stream_events = vec![json!({
        "type": "response.completed",
        "response": {
            "id": "resp_stream_tool_1",
            "status": "completed",
            "output": [{
                "type": "function_call",
                "call_id": "call_stream_tool_1",
                "name": "demo_echo",
                "arguments": "{\"text\":\"hello\"}"
            }]
        }
    })];
    let third_stream_events = vec![
        json!({ "type": "response.output_text.delta", "delta": "stream " }),
        json!({ "type": "response.output_text.delta", "delta": "tool recovery success" }),
        json!({
            "type": "response.completed",
            "response": {
                "id": "resp_stream_tool_done",
                "status": "completed",
                "output_text": "stream tool recovery success"
            }
        }),
    ];
    let steps = vec![
        MockProviderStep::sse(first_stream_events),
        MockProviderStep::sse(third_stream_events),
    ];
    let (base_url, captured, server) = start_mock_provider(steps).await;
    let mut client = build_test_client(base_url);

    let callbacks = chunk_callbacks();

    let result = run_process_with_tools(
        &mut client,
        RunProcessWithToolsArgs {
            previous_response_id: Some("prev_resp_stream_seed".to_string()),
            tools: vec![demo_echo_tool()],
            callbacks,
            use_prev_id: true,
            can_use_prev_id: true,
            ..Default::default()
        },
    )
    .await
    .expect("stream mode should recover missing tool-call context");
    server.abort();

    assert_eq!(
        result.get("content").and_then(|value| value.as_str()),
        Some("stream tool recovery success")
    );

    let requests = captured.lock().await.clone();
    assert_eq!(requests.len(), 2);

    assert_eq!(
        requests[0]
            .get("previous_response_id")
            .and_then(|value| value.as_str()),
        Some("prev_resp_stream_seed")
    );
    assert!(requests[0]
        .get("stream")
        .and_then(|value| value.as_bool())
        .unwrap_or(false));
    assert!(requests[1]
        .get("stream")
        .and_then(|value| value.as_bool())
        .unwrap_or(false));
    assert!(requests[1].get("previous_response_id").is_none());
    assert!(requests[1]
        .get("input")
        .and_then(|value| value.as_array())
        .map(|items| {
            let has_user = items
                .iter()
                .any(|item| item.get("role").and_then(|value| value.as_str()) == Some("user"));
            let has_call = items.iter().any(|item| {
                item.get("type").and_then(|value| value.as_str()) == Some("function_call")
                    && item.get("call_id").and_then(|value| value.as_str())
                        == Some("call_stream_tool_1")
            });
            let has_output = items.iter().any(|item| {
                item.get("type").and_then(|value| value.as_str()) == Some("function_call_output")
                    && item.get("call_id").and_then(|value| value.as_str())
                        == Some("call_stream_tool_1")
            });
            has_user && has_call && has_output
        })
        .unwrap_or(false));
}

#[tokio::test]
async fn recovers_stream_response_failed_missing_tool_call_without_completed_event() {
    let first_stream_events = vec![json!({
        "type": "response.completed",
        "response": {
            "id": "resp_stream_failed_seed",
            "status": "completed",
            "output": [{
                "type": "function_call",
                "call_id": "call_stream_failed_1",
                "name": "demo_echo",
                "arguments": "{\"text\":\"hello\"}"
            }]
        }
    })];
    let third_stream_events = vec![json!({
        "type": "response.completed",
        "response": {
            "id": "resp_stream_failed_done",
            "status": "completed",
            "output_text": "stream failed recovery success"
        }
    })];
    let steps = vec![
        MockProviderStep::sse(first_stream_events),
        MockProviderStep::sse(third_stream_events),
    ];
    let (base_url, captured, server) = start_mock_provider(steps).await;
    let mut client = build_test_client(base_url);

    let callbacks = chunk_callbacks();

    let result = run_process_with_tools(
        &mut client,
        RunProcessWithToolsArgs {
            previous_response_id: Some("prev_resp_stream_failed".to_string()),
            tools: vec![demo_echo_tool()],
            callbacks,
            use_prev_id: true,
            can_use_prev_id: true,
            ..Default::default()
        },
    )
    .await
    .expect("stream failed branch should recover missing tool-call context");
    server.abort();

    assert_eq!(
        result.get("content").and_then(|value| value.as_str()),
        Some("stream failed recovery success")
    );

    let requests = captured.lock().await.clone();
    assert_eq!(requests.len(), 2);

    assert_eq!(
        requests[0]
            .get("previous_response_id")
            .and_then(|value| value.as_str()),
        Some("prev_resp_stream_failed")
    );
    assert!(requests[1].get("previous_response_id").is_none());
    assert!(requests[1]
        .get("input")
        .and_then(|value| value.as_array())
        .map(|items| {
            let has_user = items
                .iter()
                .any(|item| item.get("role").and_then(|value| value.as_str()) == Some("user"));
            let has_call = items.iter().any(|item| {
                item.get("type").and_then(|value| value.as_str()) == Some("function_call")
                    && item.get("call_id").and_then(|value| value.as_str())
                        == Some("call_stream_failed_1")
            });
            let has_output = items.iter().any(|item| {
                item.get("type").and_then(|value| value.as_str()) == Some("function_call_output")
                    && item.get("call_id").and_then(|value| value.as_str())
                        == Some("call_stream_failed_1")
            });
            has_user && has_call && has_output
        })
        .unwrap_or(false));
}

#[tokio::test]
async fn recovers_stream_error_and_failed_without_status_with_pending_items() {
    let first_stream_events = vec![json!({
        "type": "response.completed",
        "response": {
            "id": "resp_stream_mix_seed",
            "status": "completed",
            "output": [{
                "type": "function_call",
                "call_id": "call_stream_mix_1",
                "name": "demo_echo",
                "arguments": "{\"text\":\"hello\"}"
            }]
        }
    })];
    let third_stream_events = vec![json!({
        "type": "response.completed",
        "response": {
            "id": "resp_stream_mix_done",
            "status": "completed",
            "output_text": "stream mixed failure recovery success"
        }
    })];
    let steps = vec![
        MockProviderStep::sse(first_stream_events),
        MockProviderStep::sse(third_stream_events),
    ];
    let (base_url, captured, server) = start_mock_provider(steps).await;
    let mut client = build_test_client(base_url);

    let callbacks = chunk_callbacks();

    let result = run_process_with_tools(
        &mut client,
        RunProcessWithToolsArgs {
            previous_response_id: Some("prev_resp_stream_mix".to_string()),
            tools: vec![demo_echo_tool()],
            callbacks,
            use_prev_id: true,
            can_use_prev_id: true,
            ..Default::default()
        },
    )
    .await
    .expect("stream mixed failure branch should recover");
    server.abort();

    assert_eq!(
        result.get("content").and_then(|value| value.as_str()),
        Some("stream mixed failure recovery success")
    );

    let requests = captured.lock().await.clone();
    assert_eq!(requests.len(), 2);

    assert_eq!(
        requests[0]
            .get("previous_response_id")
            .and_then(|value| value.as_str()),
        Some("prev_resp_stream_mix")
    );
    assert!(requests[1].get("previous_response_id").is_none());
    assert!(requests[1]
        .get("input")
        .and_then(|value| value.as_array())
        .map(|items| {
            let has_user = items
                .iter()
                .any(|item| item.get("role").and_then(|value| value.as_str()) == Some("user"));
            let has_call = items.iter().any(|item| {
                item.get("type").and_then(|value| value.as_str()) == Some("function_call")
                    && item.get("call_id").and_then(|value| value.as_str())
                        == Some("call_stream_mix_1")
            });
            let has_output = items.iter().any(|item| {
                item.get("type").and_then(|value| value.as_str()) == Some("function_call_output")
                    && item.get("call_id").and_then(|value| value.as_str())
                        == Some("call_stream_mix_1")
            });
            has_user && has_call && has_output
        })
        .unwrap_or(false));
}

#[tokio::test]
async fn recovers_stream_with_second_tool_call_without_pending_duplication() {
    let first_stream_events = vec![json!({
        "type": "response.completed",
        "response": {
            "id": "resp_stream_round_1",
            "status": "completed",
            "output": [{
                "type": "function_call",
                "call_id": "call_stream_round_1",
                "name": "demo_echo",
                "arguments": "{\"text\":\"hello\"}"
            }]
        }
    })];
    let third_stream_events = vec![json!({
        "type": "response.completed",
        "response": {
            "id": "resp_stream_round_2",
            "status": "completed",
            "output": [{
                "type": "function_call",
                "call_id": "call_stream_round_2",
                "name": "demo_echo",
                "arguments": "{\"text\":\"again\"}"
            }]
        }
    })];
    let fourth_stream_events = vec![json!({
        "type": "response.completed",
        "response": {
            "id": "resp_stream_round_done",
            "status": "completed",
            "output_text": "stream round-trip success"
        }
    })];
    let steps = vec![
        MockProviderStep::sse(first_stream_events),
        MockProviderStep::sse(third_stream_events),
        MockProviderStep::sse(fourth_stream_events),
    ];
    let (base_url, captured, server) = start_mock_provider(steps).await;
    let mut client = build_test_client(base_url);

    let callbacks = chunk_callbacks();

    let result = run_process_with_tools(
        &mut client,
        RunProcessWithToolsArgs {
            previous_response_id: Some("prev_resp_stream_round_seed".to_string()),
            tools: vec![demo_echo_tool()],
            callbacks,
            use_prev_id: true,
            can_use_prev_id: true,
            ..Default::default()
        },
    )
    .await
    .expect("stream should recover and continue with second tool call");
    server.abort();

    assert_eq!(
        result.get("content").and_then(|value| value.as_str()),
        Some("stream round-trip success")
    );

    let requests = captured.lock().await.clone();
    assert_eq!(requests.len(), 3);
    assert!(requests[1].get("previous_response_id").is_none());
    assert!(requests[2].get("previous_response_id").is_none());

    assert!(requests[1]
        .get("input")
        .and_then(|value| value.as_array())
        .map(|items| {
            let has_user = items
                .iter()
                .any(|item| item.get("role").and_then(|value| value.as_str()) == Some("user"));
            let call_1 = items
                .iter()
                .filter(|item| {
                    item.get("type").and_then(|value| value.as_str()) == Some("function_call")
                        && item.get("call_id").and_then(|value| value.as_str())
                            == Some("call_stream_round_1")
                })
                .count();
            let output_1 = items
                .iter()
                .filter(|item| {
                    item.get("type").and_then(|value| value.as_str())
                        == Some("function_call_output")
                        && item.get("call_id").and_then(|value| value.as_str())
                            == Some("call_stream_round_1")
                })
                .count();
            has_user && call_1 == 1 && output_1 == 1
        })
        .unwrap_or(false));

    assert!(requests[2]
        .get("input")
        .and_then(|value| value.as_array())
        .map(|items| {
            let has_user = items
                .iter()
                .any(|item| item.get("role").and_then(|value| value.as_str()) == Some("user"));
            let call_1 = items
                .iter()
                .filter(|item| {
                    item.get("type").and_then(|value| value.as_str()) == Some("function_call")
                        && item.get("call_id").and_then(|value| value.as_str())
                            == Some("call_stream_round_1")
                })
                .count();
            let output_1 = items
                .iter()
                .filter(|item| {
                    item.get("type").and_then(|value| value.as_str())
                        == Some("function_call_output")
                        && item.get("call_id").and_then(|value| value.as_str())
                            == Some("call_stream_round_1")
                })
                .count();
            let call_2 = items
                .iter()
                .filter(|item| {
                    item.get("type").and_then(|value| value.as_str()) == Some("function_call")
                        && item.get("call_id").and_then(|value| value.as_str())
                            == Some("call_stream_round_2")
                })
                .count();
            let output_2 = items
                .iter()
                .filter(|item| {
                    item.get("type").and_then(|value| value.as_str())
                        == Some("function_call_output")
                        && item.get("call_id").and_then(|value| value.as_str())
                            == Some("call_stream_round_2")
                })
                .count();
            has_user && call_1 == 1 && output_1 == 1 && call_2 == 1 && output_2 == 1
        })
        .unwrap_or(false));
}

#[tokio::test]
async fn retries_parse_errors_five_times_then_succeeds() {
    let mut steps = Vec::new();
    for _ in 0..5 {
        steps.push(MockProviderStep::text(StatusCode::OK, "not-json"));
    }
    steps.push(MockProviderStep::json(
        StatusCode::OK,
        json!({
            "id": "resp_retry_parse_ok",
            "status": "completed",
            "output_text": "retry parse success"
        }),
    ));

    let (base_url, captured, server) = start_mock_provider(steps).await;
    let mut client = build_test_client(base_url);

    let result = run_process_with_tools(
        &mut client,
        RunProcessWithToolsArgs {
            callbacks: empty_callbacks(),
            purpose: "chat",
            can_use_prev_id: true,
            stable_prefix_mode: true,
            prefer_stateless: true,
            ..Default::default()
        },
    )
    .await
    .expect("should succeed after parse retries");
    server.abort();

    assert_eq!(
        result.get("content").and_then(|value| value.as_str()),
        Some("retry parse success")
    );

    let requests = captured.lock().await.clone();
    assert_eq!(requests.len(), 6);
}

#[tokio::test]
async fn fails_after_five_network_retries_with_explicit_message() {
    let mut steps = Vec::new();
    for _ in 0..6 {
        steps.push(MockProviderStep::text(
            StatusCode::SERVICE_UNAVAILABLE,
            "temporary upstream outage",
        ));
    }

    let (base_url, captured, server) = start_mock_provider(steps).await;
    let mut client = build_test_client(base_url);

    let err = run_process_with_tools(
        &mut client,
        RunProcessWithToolsArgs {
            callbacks: empty_callbacks(),
            purpose: "chat",
            can_use_prev_id: true,
            stable_prefix_mode: true,
            prefer_stateless: true,
            ..Default::default()
        },
    )
    .await
    .expect_err("should fail after retry budget exhausted");
    server.abort();

    assert!(err.contains("已重试 5 次"), "{err}");
    assert!(err.contains("网络波动"), "{err}");

    let requests = captured.lock().await.clone();
    assert_eq!(requests.len(), 6);
}

#[tokio::test]
async fn retries_stream_parse_failure_and_then_succeeds() {
    let first_stream_events: Vec<Value> = vec![];
    let second_stream_events = vec![json!({
        "type": "response.completed",
        "response": {
            "id": "resp_stream_retry_parse",
            "status": "completed",
            "output_text": "stream parse retry success"
        }
    })];
    let steps = vec![
        MockProviderStep::sse(first_stream_events),
        MockProviderStep::sse(second_stream_events),
    ];
    let (base_url, captured, server) = start_mock_provider(steps).await;
    let mut client = build_test_client(base_url);

    let callbacks = chunk_callbacks();

    let result = run_process_with_tools(
        &mut client,
        RunProcessWithToolsArgs {
            callbacks,
            purpose: "chat",
            can_use_prev_id: true,
            stable_prefix_mode: true,
            prefer_stateless: true,
            ..Default::default()
        },
    )
    .await
    .expect("stream parse retry should succeed");
    server.abort();

    assert_eq!(
        result.get("content").and_then(|value| value.as_str()),
        Some("stream parse retry success")
    );

    let requests = captured.lock().await.clone();
    assert_eq!(requests.len(), 2);
}

#[tokio::test]
async fn retries_non_terminal_empty_stream_response_and_falls_back_from_prev_id() {
    let first_stream_events = vec![json!({
        "type": "response.completed",
        "response": {
            "id": "resp_non_terminal_empty",
            "status": "in_progress"
        }
    })];
    let second_stream_events = vec![json!({
        "type": "response.completed",
        "response": {
            "id": "resp_non_terminal_done",
            "status": "completed",
            "output_text": "non terminal recovery success"
        }
    })];
    let steps = vec![
        MockProviderStep::sse(first_stream_events),
        MockProviderStep::sse(second_stream_events),
    ];
    let (base_url, captured, server) = start_mock_provider(steps).await;
    let mut client = build_test_client(base_url);

    let callbacks = chunk_callbacks();

    let result = run_process_with_tools(
        &mut client,
        RunProcessWithToolsArgs {
            previous_response_id: Some("prev_resp_non_terminal_seed".to_string()),
            callbacks,
            purpose: "chat",
            use_prev_id: true,
            can_use_prev_id: true,
            stable_prefix_mode: true,
            ..Default::default()
        },
    )
    .await
    .expect("should recover from non-terminal empty response");
    server.abort();

    assert_eq!(
        result.get("content").and_then(|value| value.as_str()),
        Some("non terminal recovery success")
    );

    let requests = captured.lock().await.clone();
    assert_eq!(requests.len(), 2);
    assert_eq!(
        requests[0]
            .get("previous_response_id")
            .and_then(|value| value.as_str()),
        Some("prev_resp_non_terminal_seed")
    );
    assert!(requests[1].get("previous_response_id").is_none());
    assert!(requests[1]
        .get("input")
        .map(|value| value.is_array())
        .unwrap_or(false));
}

#[tokio::test]
async fn retries_terminal_empty_stream_response_and_recovers_with_stateless_retry() {
    let first_stream_events = vec![json!({
        "type": "response.completed",
        "response": {
            "id": "resp_terminal_empty",
            "status": "completed"
        }
    })];
    let second_stream_events = vec![json!({
        "type": "response.completed",
        "response": {
            "id": "resp_terminal_recovered",
            "status": "completed",
            "output_text": "terminal empty recovery success"
        }
    })];
    let steps = vec![
        MockProviderStep::sse(first_stream_events),
        MockProviderStep::sse(second_stream_events),
    ];
    let (base_url, captured, server) = start_mock_provider(steps).await;
    let mut client = build_test_client(base_url);

    let result = run_process_with_tools(
        &mut client,
        RunProcessWithToolsArgs {
            previous_response_id: Some("prev_resp_terminal_empty_seed".to_string()),
            callbacks: chunk_callbacks(),
            purpose: "chat",
            use_prev_id: true,
            can_use_prev_id: true,
            stable_prefix_mode: true,
            ..Default::default()
        },
    )
    .await
    .expect("should recover from terminal empty response");
    server.abort();

    assert_eq!(
        result.get("content").and_then(|value| value.as_str()),
        Some("terminal empty recovery success")
    );

    let requests = captured.lock().await.clone();
    assert_eq!(requests.len(), 2);
    assert_eq!(
        requests[0]
            .get("previous_response_id")
            .and_then(|value| value.as_str()),
        Some("prev_resp_terminal_empty_seed")
    );
    assert!(requests[1].get("previous_response_id").is_none());
    assert!(requests[1]
        .get("input")
        .map(|value| value.is_array())
        .unwrap_or(false));
}

#[tokio::test]
async fn terminal_empty_stream_response_surfaces_error_after_retry_budget_exhausted() {
    let empty_completed = vec![json!({
        "type": "response.completed",
        "response": {
            "id": "resp_terminal_empty_budget",
            "status": "completed"
        }
    })];
    let steps = vec![
        MockProviderStep::sse(empty_completed.clone()),
        MockProviderStep::sse(empty_completed.clone()),
        MockProviderStep::sse(empty_completed),
    ];
    let (base_url, captured, server) = start_mock_provider(steps).await;
    let mut client = build_test_client(base_url);

    let err = run_process_with_tools(
        &mut client,
        RunProcessWithToolsArgs {
            previous_response_id: Some("prev_resp_terminal_empty_budget".to_string()),
            callbacks: chunk_callbacks(),
            purpose: "chat",
            use_prev_id: true,
            can_use_prev_id: true,
            stable_prefix_mode: true,
            ..Default::default()
        },
    )
    .await
    .expect_err("terminal empty response should fail after retry budget");
    server.abort();

    assert!(err.contains("terminal empty response"), "{err}");

    let requests = captured.lock().await.clone();
    assert_eq!(requests.len(), 3);
}
