// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::{Arc, Mutex};

use axum::{routing::post, Json, Router};
use serde_json::{json, Value};

use super::{
    build_request_payload, build_responses_request_payload, effective_provider_for_request,
    emit_finalized_stream_callbacks, parse_timeout_seconds, validate_request_payload_size,
    AiRequestHandler, AiRequestOptions, StreamCallbacks,
};
use crate::stream_parse::FinalizedStreamState;

#[test]
fn responses_effort_respects_model_contract_for_every_provider_and_preserves_max() {
    for provider in ["gpt", "openai", "openai_compatible", "my_api"] {
        for model in ["gpt-5.6-sol", "gpt-6.1-sol"] {
            for (selected, expected) in [
                ("none", "low"),
                ("minimal", "low"),
                ("low", "low"),
                ("medium", "medium"),
                ("high", "high"),
                ("xhigh", "xhigh"),
                ("max", "max"),
            ] {
                let payload = build_request_payload(
                    json!([{"role": "user", "content": "你好"}]),
                    model.into(),
                    None,
                    None,
                    None,
                    None,
                    Some(provider.into()),
                    Some(selected.into()),
                    &AiRequestOptions::default(),
                );
                assert_eq!(payload.pointer("/reasoning/effort"), Some(&json!(expected)));
            }
        }
    }
}

#[test]
fn responses_payload_can_enable_server_side_compaction() {
    let payload = build_request_payload(
        json!([{"role": "user", "content": "hello"}]),
        "gpt-5.6-sol".to_string(),
        None,
        None,
        None,
        None,
        Some("gpt".to_string()),
        Some("high".to_string()),
        &AiRequestOptions {
            responses_compaction_threshold: Some(200_000),
            ..AiRequestOptions::default()
        },
    );

    assert_eq!(
        payload.pointer("/context_management/0/type"),
        Some(&json!("compaction"))
    );
    assert_eq!(
        payload.pointer("/context_management/0/compact_threshold"),
        Some(&json!(200_000))
    );
}

async fn delayed_provider_response() -> Json<Value> {
    tokio::time::sleep(std::time::Duration::from_secs(10)).await;
    Json(json!({
        "id": "response-delayed",
        "status": "completed",
        "output_text": "late",
        "output": []
    }))
}

async fn start_request_test_server(app: Router) -> (String, tokio::task::JoinHandle<()>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind request test server");
    let address = listener.local_addr().expect("request test address");
    let server = tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    (format!("http://{address}"), server)
}

#[test]
fn explicit_read_timeout_overrides_the_environment_default() {
    let handler = AiRequestHandler::new_with_read_timeout(std::time::Duration::from_secs(123));
    assert_eq!(handler.read_timeout_seconds(), Some(123));
}

#[tokio::test]
async fn read_timeout_interrupts_a_provider_that_makes_no_progress() {
    let (base_url, server) = start_request_test_server(
        Router::new().route("/responses", post(delayed_provider_response)),
    )
    .await;
    let handler = AiRequestHandler::new_with_read_timeout(std::time::Duration::from_millis(50));
    let started_at = std::time::Instant::now();
    let error = handler
        .handle_request(
            base_url.as_str(),
            "test-key",
            json!([{"role": "user", "content": "hello"}]),
            "gpt-test".to_string(),
            None,
            None,
            None,
            None,
            StreamCallbacks::default(),
            Some("openai".to_string()),
            None,
        )
        .await
        .expect_err("provider inactivity should time out");
    server.abort();

    assert!(error.contains("AI transport error (kind=timeout)"));
    assert!(started_at.elapsed() < std::time::Duration::from_secs(1));
}

#[tokio::test]
async fn cancellation_interrupts_a_pending_provider_request_immediately() {
    let (base_url, server) = start_request_test_server(
        Router::new().route("/responses", post(delayed_provider_response)),
    )
    .await;
    let handler = AiRequestHandler::new_with_read_timeout(std::time::Duration::from_secs(30));
    let abort_token = tokio_util::sync::CancellationToken::new();
    let cancellation = abort_token.clone();
    tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
        cancellation.cancel();
    });
    let started_at = std::time::Instant::now();
    let error = handler
        .handle_request_with_options(
            base_url.as_str(),
            "test-key",
            json!("hello"),
            "gpt-test".to_string(),
            None,
            None,
            None,
            None,
            Some("openai".to_string()),
            None,
            AiRequestOptions {
                abort_token: Some(abort_token),
                ..AiRequestOptions::default()
            },
        )
        .await
        .expect_err("cancelled provider request should abort");
    server.abort();

    assert_eq!(error, "aborted");
    assert!(started_at.elapsed() < std::time::Duration::from_secs(1));
}

#[test]
fn responses_payload_supports_prompt_cache_and_cwd() {
    let options = AiRequestOptions {
        callbacks: StreamCallbacks::default(),
        prompt_cache_key: Some("session_1".to_string()),
        previous_response_id: Some("resp_1".to_string()),
        request_cwd: Some("/workspace".to_string()),
        include_prompt_cache_retention: true,
        request_body_limit_bytes: None,
        abort_token: None,
        force_identity_encoding: false,
        on_before_send_model_request: None,
        stream: true,
        output_format: None,
        responses_compaction_threshold: None,
    };
    let payload = build_responses_request_payload(
        json!([]),
        "gpt-4.1".to_string(),
        Some("system".to_string()),
        None,
        None,
        None,
        Some("gpt".to_string()),
        Some("medium".to_string()),
        &options,
    );

    assert_eq!(
        payload.get("prompt_cache_key"),
        Some(&Value::String("session_1".to_string()))
    );
    assert_eq!(
        payload.get("prompt_cache_retention"),
        Some(&Value::String("24h".to_string()))
    );
    assert!(payload.get("previous_response_id").is_none());
    assert_eq!(
        payload.get("cwd"),
        Some(&Value::String("/workspace".to_string()))
    );
}

#[test]
fn ai_read_timeout_defaults_to_three_minutes_and_accepts_valid_override() {
    assert_eq!(parse_timeout_seconds(None, 180), 180);
    assert_eq!(parse_timeout_seconds(Some("450"), 180), 450);
    assert_eq!(parse_timeout_seconds(Some("7200"), 180), 7_200);
    assert_eq!(parse_timeout_seconds(Some("7201"), 180), 180);
    assert_eq!(parse_timeout_seconds(Some("0"), 180), 180);
    assert_eq!(parse_timeout_seconds(Some("invalid"), 180), 180);
}

#[test]
fn responses_payload_normalizes_legacy_text_parts_by_message_role() {
    let payload = build_responses_request_payload(
        json!([
            {
                "type": "message",
                "role": "system",
                "content": [{"type": "text", "text": "system context"}]
            },
            {
                "type": "message",
                "role": "user",
                "content": [{"type": "text", "text": "hello"}]
            },
            {
                "type": "message",
                "role": "assistant",
                "content": [{"type": "text", "text": "previous reply"}]
            }
        ]),
        "gpt-5.4".to_string(),
        None,
        None,
        None,
        None,
        Some("openai".to_string()),
        None,
        &AiRequestOptions::default(),
    );

    assert_eq!(
        payload.pointer("/input/0/content/0/type"),
        Some(&Value::String("input_text".to_string()))
    );
    assert_eq!(
        payload.pointer("/input/1/content/0/type"),
        Some(&Value::String("input_text".to_string()))
    );
    assert_eq!(
        payload.pointer("/input/2/content/0/type"),
        Some(&Value::String("output_text".to_string()))
    );
}

#[test]
fn responses_payload_requests_summary_for_gpt_model_on_compatible_provider() {
    let payload = build_responses_request_payload(
        json!([]),
        "gpt-5.4".to_string(),
        None,
        None,
        None,
        None,
        Some("openai_compatible".to_string()),
        Some("xhigh".to_string()),
        &AiRequestOptions::default(),
    );

    assert_eq!(
        payload.pointer("/reasoning/effort"),
        Some(&Value::String("xhigh".to_string()))
    );
    assert_eq!(
        payload.pointer("/reasoning/summary"),
        Some(&Value::String("auto".to_string()))
    );
}

#[test]
fn responses_payload_omits_summary_for_generic_compatible_model() {
    let payload = build_responses_request_payload(
        json!([]),
        "generic-compatible-model".to_string(),
        None,
        None,
        None,
        None,
        Some("openai_compatible".to_string()),
        Some("high".to_string()),
        &AiRequestOptions::default(),
    );

    assert_eq!(
        payload.pointer("/reasoning/effort"),
        Some(&Value::String("high".to_string()))
    );
    assert!(payload.pointer("/reasoning/summary").is_none());
}

#[test]
fn custom_openai_base_url_preserves_configured_provider() {
    assert_eq!(
        effective_provider_for_request(
            "https://gateway.example.test/v1",
            Some("openai".to_string()),
        )
        .as_deref(),
        Some("openai")
    );
    assert_eq!(
        effective_provider_for_request("https://api.openai.com/v1", Some("openai".to_string()),)
            .as_deref(),
        Some("openai")
    );
}

#[test]
fn request_payload_size_limit_rejects_oversized_body() {
    let err = validate_request_payload_size(129, Some(128)).expect_err("should reject");
    assert_eq!(
        err,
        "AI request payload too large: 129 bytes exceeds 128 bytes"
    );
}

#[test]
fn request_payload_size_limit_allows_unset_or_zero_limit() {
    assert!(validate_request_payload_size(usize::MAX, None).is_ok());
    assert!(validate_request_payload_size(usize::MAX, Some(0)).is_ok());
}

#[tokio::test]
async fn transport_errors_preserve_error_kind_and_source_chain() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind unused port");
    let address = listener.local_addr().expect("unused port address");
    drop(listener);

    let error = AiRequestHandler::new()
        .handle_request(
            format!("http://{address}").as_str(),
            "test-key",
            json!({"messages": []}),
            "test-model".to_string(),
            None,
            None,
            None,
            None,
            StreamCallbacks::default(),
            Some("openai_compatible".to_string()),
            None,
        )
        .await
        .expect_err("closed port should fail");

    assert!(error.contains("AI transport error (kind=connect)"));
    assert!(error.contains("caused by:"));
}

#[test]
fn finalized_stream_callbacks_emit_final_reasoning_when_no_stream_thinking() {
    let thinkings = Arc::new(Mutex::new(Vec::<String>::new()));
    let callbacks = StreamCallbacks {
        on_chunk: None,
        on_thinking: Some(Arc::new({
            let thinkings = thinkings.clone();
            move |value| {
                thinkings.lock().expect("lock poisoned").push(value);
            }
        })),
    };
    let finalized = FinalizedStreamState {
        content: "done".to_string(),
        reasoning: Some("final reasoning".to_string()),
        ..FinalizedStreamState::default()
    };

    emit_finalized_stream_callbacks(&finalized, true, false, &callbacks);

    assert_eq!(
        thinkings.lock().expect("lock poisoned").as_slice(),
        ["final reasoning"]
    );
}

#[test]
fn finalized_stream_callbacks_do_not_duplicate_streamed_thinking() {
    let thinkings = Arc::new(Mutex::new(Vec::<String>::new()));
    let callbacks = StreamCallbacks {
        on_chunk: None,
        on_thinking: Some(Arc::new({
            let thinkings = thinkings.clone();
            move |value| {
                thinkings.lock().expect("lock poisoned").push(value);
            }
        })),
    };
    let finalized = FinalizedStreamState {
        content: "done".to_string(),
        reasoning: Some("final reasoning".to_string()),
        ..FinalizedStreamState::default()
    };

    emit_finalized_stream_callbacks(&finalized, true, true, &callbacks);

    assert!(thinkings.lock().expect("lock poisoned").is_empty());
}
