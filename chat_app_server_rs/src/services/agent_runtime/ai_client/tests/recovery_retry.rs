use super::*;

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
            stable_prefix_mode: true,
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
            stable_prefix_mode: true,
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
            stable_prefix_mode: true,
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
async fn retries_non_terminal_empty_stream_response_and_recovers_statelessly() {
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
            callbacks,
            purpose: "chat",
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
    assert!(requests[0].get("prev_id").is_none());
    assert!(requests[1].get("prev_id").is_none());
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
            callbacks: chunk_callbacks(),
            purpose: "chat",
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
    assert!(requests[0].get("prev_id").is_none());
    assert!(requests[1].get("prev_id").is_none());
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
            callbacks: chunk_callbacks(),
            purpose: "chat",
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
