// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

#[tokio::test]
async fn session_can_switch_from_responses_to_chat_completions() {
    let session_id = unique_session_id("session_transport_switch_r2c");
    ensure_memory_session(session_id.as_str())
        .await
        .expect("setup transport switch session");

    let first_steps = vec![MockProviderStep::json(
        StatusCode::OK,
        json!({
            "id": "resp_switch_seed",
            "status": "completed",
            "output_text": "responses seed"
        }),
    )];
    let (first_base_url, first_captured, first_server) = start_mock_provider(first_steps).await;
    let mut first_client = build_test_client(first_base_url);

    let first_result = run_process_with_tools(
        &mut first_client,
        RunProcessWithToolsArgs {
            session_id: Some(session_id.clone()),
            prompt_cache_key: Some(session_id.clone()),
            callbacks: empty_callbacks(),
            purpose: "chat",
            stable_prefix_mode: true,
            supports_responses: true,
            ..Default::default()
        },
    )
    .await
    .expect("responses seed request should succeed");
    first_server.abort();

    assert_eq!(
        first_result.get("content").and_then(|value| value.as_str()),
        Some("responses seed")
    );

    let second_steps = vec![MockProviderStep::json(
        StatusCode::OK,
        json!({
            "id": "chatcmpl-switch-follow-up",
            "choices": [{
                "message": {"role": "assistant", "content": "chat follow-up"},
                "finish_reason": "stop"
            }],
            "usage": {"prompt_tokens": 10, "completion_tokens": 5, "total_tokens": 15}
        }),
    )];
    let (second_base_url, second_captured, second_server) = start_mock_provider(second_steps).await;
    let mut second_client = build_test_client(second_base_url);

    let second_result = run_process_with_tools(
        &mut second_client,
        RunProcessWithToolsArgs {
            session_id: Some(session_id.clone()),
            callbacks: empty_callbacks(),
            purpose: "chat",
            stable_prefix_mode: true,
            supports_responses: false,
            ..Default::default()
        },
    )
    .await
    .expect("chat completions follow-up should succeed");
    second_server.abort();

    assert_eq!(
        second_result
            .get("content")
            .and_then(|value| value.as_str()),
        Some("chat follow-up")
    );

    let first_requests = first_captured.lock().await.clone();
    assert_eq!(first_requests.len(), 1);
    assert!(first_requests[0].get("input").is_some());

    let second_requests = second_captured.lock().await.clone();
    assert_eq!(second_requests.len(), 1);
    let second_messages = second_requests[0]
        .get("messages")
        .and_then(|value| value.as_array())
        .expect("chat-completions messages");
    assert!(second_messages.iter().any(|message| {
        message.get("role").and_then(|value| value.as_str()) == Some("assistant")
            && message
                .get("content")
                .map(|value| value.to_string().contains("responses seed"))
                .unwrap_or(false)
    }));
}

#[tokio::test]
async fn session_can_switch_from_chat_completions_to_responses() {
    let session_id = unique_session_id("session_transport_switch_c2r");
    ensure_memory_session(session_id.as_str())
        .await
        .expect("setup transport switch session");

    let first_steps = vec![MockProviderStep::json(
        StatusCode::OK,
        json!({
            "id": "chatcmpl-switch-seed",
            "choices": [{
                "message": {"role": "assistant", "content": "chat seed"},
                "finish_reason": "stop"
            }],
            "usage": {"prompt_tokens": 8, "completion_tokens": 4, "total_tokens": 12}
        }),
    )];
    let (first_base_url, first_captured, first_server) = start_mock_provider(first_steps).await;
    let mut first_client = build_test_client(first_base_url);

    let first_result = run_process_with_tools(
        &mut first_client,
        RunProcessWithToolsArgs {
            session_id: Some(session_id.clone()),
            callbacks: empty_callbacks(),
            purpose: "chat",
            stable_prefix_mode: true,
            supports_responses: false,
            ..Default::default()
        },
    )
    .await
    .expect("chat-completions seed request should succeed");
    first_server.abort();

    assert_eq!(
        first_result.get("content").and_then(|value| value.as_str()),
        Some("chat seed")
    );

    let second_steps = vec![MockProviderStep::json(
        StatusCode::OK,
        json!({
            "id": "resp_switch_follow_up",
            "status": "completed",
            "output_text": "responses follow-up"
        }),
    )];
    let (second_base_url, second_captured, second_server) = start_mock_provider(second_steps).await;
    let mut second_client = build_test_client(second_base_url);

    let second_result = run_process_with_tools(
        &mut second_client,
        RunProcessWithToolsArgs {
            session_id: Some(session_id.clone()),
            prompt_cache_key: Some(session_id.clone()),
            callbacks: empty_callbacks(),
            purpose: "chat",
            stable_prefix_mode: true,
            supports_responses: true,
            ..Default::default()
        },
    )
    .await
    .expect("responses follow-up should succeed");
    second_server.abort();

    assert_eq!(
        second_result
            .get("content")
            .and_then(|value| value.as_str()),
        Some("responses follow-up")
    );

    let first_requests = first_captured.lock().await.clone();
    assert_eq!(first_requests.len(), 1);
    assert!(first_requests[0].get("messages").is_some());

    let second_requests = second_captured.lock().await.clone();
    assert_eq!(second_requests.len(), 1);
    let second_input = second_requests[0]
        .get("input")
        .and_then(|value| value.as_array())
        .expect("responses input array");
    assert!(second_input.iter().any(|item| {
        item.get("role").and_then(|value| value.as_str()) == Some("assistant")
            && item
                .get("content")
                .map(|value| value.to_string().contains("chat seed"))
                .unwrap_or(false)
    }));
}
