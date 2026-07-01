// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

#[tokio::test]
async fn tool_follow_up_uses_stateless_tool_context_outputs() {
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
            tools: vec![demo_echo_tool()],
            callbacks: empty_callbacks(),
            ..Default::default()
        },
    )
    .await
    .expect("tool follow-up should run with stateless context");
    server.abort();

    assert_eq!(
        result.get("content").and_then(|value| value.as_str()),
        Some("tool recovery success")
    );

    let requests = captured.lock().await.clone();
    assert_eq!(requests.len(), 2);
    assert!(requests
        .iter()
        .all(|request| request.get("prev_id").is_none()));
    assert!(requests[1]
        .get("input")
        .and_then(|value| value.as_array())
        .map(|items| {
            let has_output = items.iter().any(|item| {
                item.get("type").and_then(|value| value.as_str()) == Some("function_call_output")
                    && item.get("call_id").and_then(|value| value.as_str()) == Some("call_tool_1")
            });
            has_output
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
            tools: vec![demo_echo_tool()],
            callbacks: empty_callbacks(),
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
    assert!(requests[0].get("prev_id").is_none());
    assert!(requests[1].get("prev_id").is_none());
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
            tools: vec![demo_echo_tool()],
            callbacks: empty_callbacks(),
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
    assert!(requests[1].get("prev_id").is_none());
    assert!(requests[2].get("prev_id").is_none());
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
async fn keeps_stateless_mode_after_missing_tool_call_fallback() {
    let session_id = unique_session_id("session_prev_id_missing_tool_call");
    ensure_memory_session(session_id.as_str())
        .await
        .expect("setup session for prev-id disable test");

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
                "output_text": "tool prev-id stays stateless"
            }),
        ),
    ];
    let (base_url, captured, server) = start_mock_provider(steps).await;
    let mut client = build_test_client(base_url);

    let result = run_process_with_tools(
        &mut client,
        RunProcessWithToolsArgs {
            session_id: Some(session_id),
            tools: vec![demo_echo_tool()],
            callbacks: empty_callbacks(),
            ..Default::default()
        },
    )
    .await
    .expect("missing tool-call fallback should keep stateless mode");
    server.abort();

    assert_eq!(
        result.get("content").and_then(|value| value.as_str()),
        Some("tool prev-id stays stateless")
    );

    let requests = captured.lock().await.clone();
    assert_eq!(requests.len(), 4);
    assert!(requests[1].get("prev_id").is_none());
    assert!(requests[2].get("prev_id").is_none());
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
    assert!(requests[3].get("prev_id").is_none());
    assert!(requests[3]
        .get("input")
        .and_then(|value| value.as_array())
        .map(|items| {
            let has_user = items
                .iter()
                .any(|item| item.get("role").and_then(|value| value.as_str()) == Some("user"));
            let has_call = items.iter().any(|item| {
                item.get("type").and_then(|value| value.as_str()) == Some("function_call")
                    && item.get("call_id").and_then(|value| value.as_str())
                        == Some("call_tool_prev_id_recovered")
            });
            let has_output = items.iter().any(|item| {
                item.get("type").and_then(|value| value.as_str()) == Some("function_call_output")
                    && item.get("call_id").and_then(|value| value.as_str())
                        == Some("call_tool_prev_id_recovered")
            });
            has_user && has_call && has_output
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
            tools: vec![demo_echo_tool()],
            callbacks,
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

    assert!(requests[0].get("prev_id").is_none());
    assert!(requests[0]
        .get("stream")
        .and_then(|value| value.as_bool())
        .unwrap_or(false));
    assert!(requests[1]
        .get("stream")
        .and_then(|value| value.as_bool())
        .unwrap_or(false));
    assert!(requests[1].get("prev_id").is_none());
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
            tools: vec![demo_echo_tool()],
            callbacks,
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

    assert!(requests[0].get("prev_id").is_none());
    assert!(requests[1].get("prev_id").is_none());
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
            tools: vec![demo_echo_tool()],
            callbacks,
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

    assert!(requests[0].get("prev_id").is_none());
    assert!(requests[1].get("prev_id").is_none());
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
    let session_id = unique_session_id("session_stream_round_recovery");
    ensure_memory_session(session_id.as_str())
        .await
        .expect("setup session for stream round recovery");

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
            session_id: Some(session_id),
            tools: vec![demo_echo_tool()],
            callbacks,
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
    assert!(requests
        .iter()
        .all(|request| request.get("prev_id").is_none()));

    assert!(requests[1]
        .get("input")
        .and_then(|value| value.as_array())
        .map(|items| {
            let has_output_1 = items.iter().any(|item| {
                item.get("type").and_then(|value| value.as_str()) == Some("function_call_output")
                    && item.get("call_id").and_then(|value| value.as_str())
                        == Some("call_stream_round_1")
            });
            has_output_1
        })
        .unwrap_or(false));

    assert!(requests[2]
        .get("input")
        .and_then(|value| value.as_array())
        .map(|items| {
            let has_output_2 = items.iter().any(|item| {
                item.get("type").and_then(|value| value.as_str()) == Some("function_call_output")
                    && item.get("call_id").and_then(|value| value.as_str())
                        == Some("call_stream_round_2")
            });
            has_output_2
        })
        .unwrap_or(false));
}
