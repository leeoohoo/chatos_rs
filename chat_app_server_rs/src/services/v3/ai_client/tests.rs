use axum::http::StatusCode;
use serde_json::{json, Value};

use super::test_support::{
    build_test_client, chunk_callbacks, demo_echo_tool, empty_callbacks, run_process_with_tools,
    start_mock_provider, MockProviderStep, RunProcessWithToolsArgs,
};

#[tokio::test]
async fn recovers_prev_id_then_completion_overflow_and_succeeds() {
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
        MockProviderStep::json(
            StatusCode::OK,
            json!({
                "id": "resp_ok",
                "status": "completed",
                "output_text": "final answer"
            }),
        ),
    ];
    let (base_url, captured, server) = start_mock_provider(steps).await;
    let mut client = build_test_client(base_url);

    let result = run_process_with_tools(
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
    .expect("process should recover and succeed");
    server.abort();

    assert_eq!(
        result.get("content").and_then(|value| value.as_str()),
        Some("final answer")
    );

    let requests = captured.lock().await.clone();
    assert_eq!(requests.len(), 3);
    assert!(requests[0].get("previous_response_id").is_some());
    assert!(requests[1].get("previous_response_id").is_none());
    assert!(requests[2].get("previous_response_id").is_none());
    assert!(requests[1]
        .get("input")
        .map(|value| value.is_array())
        .unwrap_or(false));
    assert!(requests[2]
        .get("input")
        .map(|value| value.is_array())
        .unwrap_or(false));
}

#[tokio::test]
async fn prefixed_runtime_items_disable_previous_response_id_reuse() {
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
                        "text": "联系人 runtime context"
                    }
                ]
            })],
            stable_prefix_mode: true,
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
    let request_input_text = requests[0]
        .get("input")
        .map(|value| value.to_string())
        .unwrap_or_default();
    assert!(request_input_text.contains("联系人 runtime context"));
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
async fn recovers_missing_tool_call_output_with_pending_tool_items_merged() {
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
        MockProviderStep::text(
            StatusCode::BAD_REQUEST,
            "No tool call found for function_call_output item",
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
    .expect("process should recover missing tool-call context");
    server.abort();

    assert_eq!(
        result.get("content").and_then(|value| value.as_str()),
        Some("tool recovery success")
    );

    let requests = captured.lock().await.clone();
    assert_eq!(requests.len(), 3);

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
            items.iter().all(|item| {
                item.get("type").and_then(|value| value.as_str()) == Some("function_call_output")
            })
        })
        .unwrap_or(false));

    assert!(requests[2].get("previous_response_id").is_none());
    assert!(requests[2]
        .get("input")
        .and_then(|value| value.as_array())
        .map(|items| {
            let has_call = items.iter().any(|item| {
                item.get("type").and_then(|value| value.as_str()) == Some("function_call")
                    && item.get("call_id").and_then(|value| value.as_str()) == Some("call_tool_1")
            });
            let has_output = items.iter().any(|item| {
                item.get("type").and_then(|value| value.as_str()) == Some("function_call_output")
                    && item.get("call_id").and_then(|value| value.as_str()) == Some("call_tool_1")
            });
            has_call && has_output
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
        MockProviderStep::text(
            StatusCode::BAD_REQUEST,
            "No tool call found for function_call_output item",
        ),
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
    assert_eq!(requests.len(), 3);

    assert_eq!(
        requests[0]
            .get("previous_response_id")
            .and_then(|value| value.as_str()),
        Some("prev_resp_stream_seed")
    );
    assert_eq!(
        requests[1]
            .get("previous_response_id")
            .and_then(|value| value.as_str()),
        Some("resp_stream_tool_1")
    );
    assert!(requests[0]
        .get("stream")
        .and_then(|value| value.as_bool())
        .unwrap_or(false));
    assert!(requests[1]
        .get("stream")
        .and_then(|value| value.as_bool())
        .unwrap_or(false));
    assert!(requests[2]
        .get("stream")
        .and_then(|value| value.as_bool())
        .unwrap_or(false));

    assert!(requests[2].get("previous_response_id").is_none());
    assert!(requests[2]
        .get("input")
        .and_then(|value| value.as_array())
        .map(|items| {
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
            has_call && has_output
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
    let second_stream_events = vec![json!({
        "type": "response.failed",
        "response": {
            "id": "resp_stream_failed_mid",
            "status": "failed",
            "error": {
                "message": "No tool call found for function_call_output item"
            }
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
        MockProviderStep::sse(second_stream_events),
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
    assert_eq!(requests.len(), 3);

    assert_eq!(
        requests[0]
            .get("previous_response_id")
            .and_then(|value| value.as_str()),
        Some("prev_resp_stream_failed")
    );
    assert_eq!(
        requests[1]
            .get("previous_response_id")
            .and_then(|value| value.as_str()),
        Some("resp_stream_failed_seed")
    );
    assert!(requests[2].get("previous_response_id").is_none());

    assert!(requests[1]
        .get("input")
        .and_then(|value| value.as_array())
        .map(|items| {
            items.iter().all(|item| {
                item.get("type").and_then(|value| value.as_str()) == Some("function_call_output")
            })
        })
        .unwrap_or(false));
    assert!(requests[2]
        .get("input")
        .and_then(|value| value.as_array())
        .map(|items| {
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
            has_call && has_output
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
    let second_stream_events = vec![
        json!({
            "type": "error",
            "error": {
                "message": "No tool call found for function_call_output item"
            }
        }),
        json!({
            "type": "response.failed",
            "response": {
                "id": "resp_stream_mix_mid"
            }
        }),
    ];
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
        MockProviderStep::sse(second_stream_events),
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
    assert_eq!(requests.len(), 3);

    assert_eq!(
        requests[0]
            .get("previous_response_id")
            .and_then(|value| value.as_str()),
        Some("prev_resp_stream_mix")
    );
    assert_eq!(
        requests[1]
            .get("previous_response_id")
            .and_then(|value| value.as_str()),
        Some("resp_stream_mix_seed")
    );
    assert!(requests[2].get("previous_response_id").is_none());
    assert!(requests[2]
        .get("input")
        .and_then(|value| value.as_array())
        .map(|items| {
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
            has_call && has_output
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
    let second_stream_events = vec![
        json!({
            "type": "error",
            "error": {
                "message": "No tool call found for function_call_output item"
            }
        }),
        json!({
            "type": "response.failed",
            "response": {
                "id": "resp_stream_round_fail"
            }
        }),
    ];
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
        MockProviderStep::sse(second_stream_events),
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
    assert_eq!(requests.len(), 4);

    assert_eq!(
        requests[1]
            .get("previous_response_id")
            .and_then(|value| value.as_str()),
        Some("resp_stream_round_1")
    );
    assert!(requests[2].get("previous_response_id").is_none());
    assert!(requests[3].get("previous_response_id").is_none());

    assert!(requests[2]
        .get("input")
        .and_then(|value| value.as_array())
        .map(|items| {
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
            call_1 == 1 && output_1 == 1
        })
        .unwrap_or(false));

    assert!(requests[3]
        .get("input")
        .and_then(|value| value.as_array())
        .map(|items| {
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
            call_1 == 1 && output_1 == 1 && call_2 == 1 && output_2 == 1
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
