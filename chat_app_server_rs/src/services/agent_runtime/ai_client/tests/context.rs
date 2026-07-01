// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

#[tokio::test]
async fn completion_overflow_without_remote_summary_surfaces_error() {
    let steps = vec![MockProviderStep::json(
        StatusCode::OK,
        json!({
            "id": "resp_failed",
            "status": "failed",
            "error": { "message": "context_length_exceeded: input exceeds the context window" }
        }),
    )];
    let (base_url, captured, server) = start_mock_provider(steps).await;
    let mut client = build_test_client(base_url);

    let err = run_process_with_tools(
        &mut client,
        RunProcessWithToolsArgs {
            callbacks: empty_callbacks(),
            ..Default::default()
        },
    )
    .await
    .expect_err("without remote summary support, overflow should surface");
    server.abort();

    assert!(err.contains("context_length_exceeded"), "{err}");

    let requests = captured.lock().await.clone();
    assert_eq!(requests.len(), 1);
    assert!(requests[0].get("prev_id").is_none());
    assert!(requests[0].get("input").is_some());
}

#[tokio::test]
async fn stable_prefixed_items_use_stateless_context() {
    let session_id = unique_session_id("session_contact_stable");
    ensure_memory_session(session_id.as_str())
        .await
        .expect("setup stable session");

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
            session_id: Some(session_id.clone()),
            prompt_cache_key: Some(session_id.clone()),
            callbacks: empty_callbacks(),
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
    .expect("stable prefixed items should preserve stateless request");
    server.abort();

    assert_eq!(
        result.get("content").and_then(|value| value.as_str()),
        Some("contact stable ok")
    );

    let requests = captured.lock().await.clone();
    assert_eq!(requests.len(), 1);
    assert!(requests[0].get("prev_id").is_none());
    assert_eq!(
        requests[0]
            .get("prompt_cache_key")
            .and_then(|value| value.as_str()),
        Some(session_id.as_str())
    );
}

#[tokio::test]
async fn relay_domain_forces_stateless_without_prev_id() {
    let steps = vec![MockProviderStep::json(
        StatusCode::OK,
        json!({
            "id": "resp_relay_stateless",
            "status": "completed",
            "output_text": "relay stateless ok"
        }),
    )];
    let (base_url, captured, server) = start_mock_provider(steps).await;
    let relay_base_url = base_url.replacen("http://", "http://relay.nf.video@", 1);
    let mut client = build_test_client(relay_base_url);

    let result = run_process_with_tools(
        &mut client,
        RunProcessWithToolsArgs {
            callbacks: empty_callbacks(),
            ..Default::default()
        },
    )
    .await
    .expect("relay domain should force stateless mode");
    server.abort();

    assert_eq!(
        result.get("content").and_then(|value| value.as_str()),
        Some("relay stateless ok")
    );

    let requests = captured.lock().await.clone();
    assert_eq!(requests.len(), 1);
    assert!(requests[0].get("prev_id").is_none());
    assert!(requests[0].get("input").is_some());
}

#[tokio::test]
async fn runtime_guidance_items_keep_stateless_mode() {
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
            callbacks: empty_callbacks(),
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
    assert!(requests[0].get("prev_id").is_none());
}

#[tokio::test]
async fn runtime_guidance_attachment_parts_are_responses_compatible() {
    let session_id = unique_session_id("session_runtime_guidance_attachment");
    let turn_id = "turn_runtime_guidance_attachment";
    ensure_memory_session(session_id.as_str())
        .await
        .expect("setup runtime guidance session");

    crate::modules::conversation_runtime::guidance::register_active_turn(
        session_id.as_str(),
        turn_id,
    );
    crate::modules::conversation_runtime::guidance::enqueue_runtime_guidance_with_attachments(
        session_id.as_str(),
        turn_id,
        "please inspect this image",
        vec![crate::utils::attachments::Attachment {
            name: Some("diagram.png".to_string()),
            mime_type: Some("image/png".to_string()),
            size: Some(32),
            data_url: Some("data:image/png;base64,Zm9v".to_string()),
            ..crate::utils::attachments::Attachment::default()
        }],
    )
    .expect("runtime guidance should enqueue");

    let steps = vec![MockProviderStep::json(
        StatusCode::OK,
        json!({
            "id": "resp_runtime_guidance_attachment",
            "status": "completed",
            "output_text": "runtime guidance ok"
        }),
    )];
    let (base_url, captured, server) = start_mock_provider(steps).await;
    let mut client = build_test_client(base_url);

    let result = run_process_with_tools(
        &mut client,
        RunProcessWithToolsArgs {
            session_id: Some(session_id.clone()),
            turn_id: Some(turn_id.to_string()),
            callbacks: empty_callbacks(),
            purpose: "chat",
            ..Default::default()
        },
    )
    .await
    .expect("runtime guidance request should succeed");
    crate::modules::conversation_runtime::guidance::close_active_turn(session_id.as_str(), turn_id);
    server.abort();

    assert_eq!(
        result.get("content").and_then(|value| value.as_str()),
        Some("runtime guidance ok")
    );

    let requests = captured.lock().await.clone();
    assert_eq!(requests.len(), 1);
    let input = requests[0]
        .get("input")
        .and_then(|value| value.as_array())
        .expect("request input should be item array");
    let guidance_item = input
        .iter()
        .find(|item| {
            item.get("role").and_then(|value| value.as_str()) == Some("user")
                && item
                    .get("content")
                    .and_then(|value| value.as_array())
                    .map(|parts| {
                        parts.iter().any(|part| {
                            part.get("text")
                                .and_then(|value| value.as_str())
                                .map(|text| text.contains("Runtime Guidance"))
                                .unwrap_or(false)
                        })
                    })
                    .unwrap_or(false)
        })
        .expect("runtime guidance message should be appended");
    let content_parts = guidance_item
        .get("content")
        .and_then(|value| value.as_array())
        .expect("runtime guidance content should be parts");
    assert!(content_parts.iter().all(|part| {
        !matches!(
            part.get("type").and_then(|value| value.as_str()),
            Some("text" | "image_url")
        )
    }));
    assert!(content_parts.iter().any(|part| {
        part.get("type").and_then(|value| value.as_str()) == Some("input_text")
            && part
                .get("text")
                .and_then(|value| value.as_str())
                .map(|text| text.contains("please inspect this image"))
                .unwrap_or(false)
    }));
    assert!(content_parts.iter().any(|part| {
        part.get("type").and_then(|value| value.as_str()) == Some("input_image")
            && part
                .get("image_url")
                .and_then(|value| value.as_str())
                .map(|url| url == "data:image/png;base64,Zm9v")
                .unwrap_or(false)
    }));
}
