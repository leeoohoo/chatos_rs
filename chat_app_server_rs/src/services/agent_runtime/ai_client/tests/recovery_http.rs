// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

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
async fn retries_stream_request_when_provider_rate_limits() {
    let steps = vec![
        MockProviderStep::json(
            StatusCode::TOO_MANY_REQUESTS,
            json!({
                "error": {
                    "message": "Rate limit exceeded",
                    "type": "bad_response_status_code",
                    "code": "bad_response_status_code"
                }
            }),
        ),
        MockProviderStep::json(
            StatusCode::OK,
            json!({
                "id": "resp_after_rate_limit",
                "status": "completed",
                "output_text": "retry after rate limit success"
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
    .expect("rate limit should retry");
    server.abort();

    assert_eq!(
        result.get("content").and_then(|value| value.as_str()),
        Some("retry after rate limit success")
    );

    let requests = captured.lock().await.clone();
    assert_eq!(requests.len(), 2);
}
