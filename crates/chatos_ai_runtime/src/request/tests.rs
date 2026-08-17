// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use axum::{extract::State, http::StatusCode, routing::post, Json, Router};
use serde_json::{json, Value};

use super::{
    build_chat_completions_request_payload, build_responses_request_payload,
    effective_provider_for_request, emit_finalized_stream_callbacks, parse_timeout_seconds,
    response_items_to_chat_messages, validate_request_payload_size, AiRequestHandler,
    AiRequestOptions, StreamCallbacks,
};
use crate::stream_parse::FinalizedStreamState;

async fn token_count_success(Json(payload): Json<Value>) -> Json<Value> {
    assert!(payload.get("tools").is_some());
    Json(json!({
        "object": "response.input_tokens",
        "input_tokens": 12_345
    }))
}

async fn token_count_unsupported(
    State(hits): State<Arc<AtomicUsize>>,
) -> (StatusCode, Json<Value>) {
    hits.fetch_add(1, Ordering::SeqCst);
    (StatusCode::NOT_FOUND, Json(json!({"error": "unsupported"})))
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

#[tokio::test]
async fn counts_responses_input_tokens_and_caches_unsupported_capability() {
    let (supported_url, supported_server) = start_request_test_server(
        Router::new().route("/responses/input_tokens", post(token_count_success)),
    )
    .await;
    let handler = AiRequestHandler::new();
    let count = handler
        .count_responses_input_tokens(
            supported_url.as_str(),
            "test-key",
            json!({
                "model": "gpt-test",
                "input": "hello",
                "tools": [{"type": "function", "name": "lookup"}]
            }),
            None,
        )
        .await
        .expect("count input tokens");
    assert_eq!(count, Some(12_345));
    supported_server.abort();

    let hits = Arc::new(AtomicUsize::new(0));
    let (unsupported_url, unsupported_server) = start_request_test_server(
        Router::new()
            .route("/responses/input_tokens", post(token_count_unsupported))
            .with_state(Arc::clone(&hits)),
    )
    .await;
    let handler = AiRequestHandler::new();
    for _ in 0..2 {
        let count = handler
            .count_responses_input_tokens(
                unsupported_url.as_str(),
                "test-key",
                json!({"model": "gpt-test", "input": "hello"}),
                None,
            )
            .await
            .expect("unsupported falls back");
        assert_eq!(count, None);
    }
    assert_eq!(hits.load(Ordering::SeqCst), 1);
    unsupported_server.abort();
}

#[test]
fn response_items_to_chat_messages_keeps_complete_tool_exchange() {
    let messages = response_items_to_chat_messages(vec![
        json!({
            "type": "message",
            "role": "assistant",
            "content": [{"type": "output_text", "text": "checking"}]
        }),
        json!({
            "type": "function_call",
            "call_id": "call_1",
            "name": "memory_search",
            "arguments": "{\"q\":\"rust\"}"
        }),
        json!({
            "type": "function_call_output",
            "call_id": "call_1",
            "output": "done"
        }),
    ]);

    assert_eq!(messages.len(), 2);
    assert_eq!(
        messages[0]
            .get("tool_calls")
            .and_then(Value::as_array)
            .map(Vec::len),
        Some(1)
    );
    assert_eq!(
        messages[1].get("role").and_then(Value::as_str),
        Some("tool")
    );
}

#[test]
fn response_items_to_chat_messages_drops_incomplete_tool_exchange() {
    let messages = response_items_to_chat_messages(vec![
        json!({
            "type": "function_call",
            "call_id": "call_1",
            "name": "memory_search",
            "arguments": "{}"
        }),
        json!({
            "type": "message",
            "role": "user",
            "content": "next"
        }),
    ]);

    assert_eq!(messages.len(), 1);
    assert_eq!(
        messages[0].get("role").and_then(Value::as_str),
        Some("user")
    );
}

#[test]
fn deepseek_thinking_chat_payload_skips_temperature() {
    let payload = build_chat_completions_request_payload(
        json!("hello"),
        "deepseek-reasoner".to_string(),
        None,
        None,
        Some(0.7),
        None,
        Some("deepseek".to_string()),
        Some("high".to_string()),
        true,
        None,
    );

    assert!(payload.get("temperature").is_none());
    assert_eq!(
        payload.get("thinking").and_then(|value| value.get("type")),
        Some(&Value::String("enabled".to_string()))
    );
    assert_eq!(
        payload.get("reasoning_effort"),
        Some(&Value::String("high".to_string()))
    );
}

#[test]
fn kimi_uses_model_specific_thinking_and_output_token_parameters() {
    let k3 = build_chat_completions_request_payload(
        json!("hello"),
        "kimi-k3".to_string(),
        None,
        None,
        Some(0.6),
        Some(4096),
        Some("kimi".to_string()),
        Some("high".to_string()),
        true,
        None,
    );
    assert_eq!(
        k3.get("reasoning_effort").and_then(Value::as_str),
        Some("high")
    );
    assert!(k3.get("thinking").is_none());
    assert_eq!(
        k3.get("max_completion_tokens").and_then(Value::as_i64),
        Some(4096)
    );
    assert!(k3.get("max_tokens").is_none());

    let k27 = build_chat_completions_request_payload(
        json!("hello"),
        "kimi-k2.7-code".to_string(),
        None,
        None,
        None,
        None,
        Some("kimi".to_string()),
        Some("none".to_string()),
        true,
        None,
    );
    assert_eq!(
        k27.get("thinking").and_then(|value| value.get("type")),
        Some(&Value::String("enabled".to_string()))
    );
    assert!(k27.get("reasoning_effort").is_none());

    let k26 = build_chat_completions_request_payload(
        json!("hello"),
        "kimi-k2.6".to_string(),
        None,
        None,
        None,
        None,
        Some("kimi".to_string()),
        Some("none".to_string()),
        true,
        None,
    );
    assert_eq!(
        k26.get("thinking").and_then(|value| value.get("type")),
        Some(&Value::String("disabled".to_string()))
    );
}

#[test]
fn glm_uses_chat_thinking_dialect_without_openai_stream_options() {
    let glm_45 = build_chat_completions_request_payload(
        json!("hello"),
        "glm-4.5-air".to_string(),
        None,
        None,
        Some(0.6),
        Some(4096),
        Some("glm".to_string()),
        Some("none".to_string()),
        true,
        None,
    );
    assert_eq!(
        glm_45.get("thinking").and_then(|value| value.get("type")),
        Some(&Value::String("disabled".to_string()))
    );
    assert!(glm_45.get("reasoning_effort").is_none());
    assert!(glm_45.get("stream_options").is_none());

    let glm_52 = build_chat_completions_request_payload(
        json!("hello"),
        "glm-5.2".to_string(),
        None,
        None,
        None,
        None,
        Some("glm".to_string()),
        Some("xhigh".to_string()),
        true,
        None,
    );
    assert_eq!(
        glm_52.get("reasoning_effort").and_then(Value::as_str),
        Some("xhigh")
    );
    assert_eq!(
        glm_52.get("thinking").and_then(|value| value.get("type")),
        Some(&Value::String("enabled".to_string()))
    );
}

#[test]
fn responses_payload_supports_prompt_cache_and_cwd() {
    let options = AiRequestOptions {
        prompt_cache_key: Some("session_1".to_string()),
        previous_response_id: Some("resp_1".to_string()),
        request_cwd: Some("/workspace".to_string()),
        include_prompt_cache_retention: true,
        request_body_limit_bytes: None,
        abort_token: None,
        force_identity_encoding: false,
        stream: true,
        output_format: None,
    };
    let payload = build_responses_request_payload(
        json!([]),
        "gpt-4.1".to_string(),
        Some("system".to_string()),
        options.prompt_cache_key,
        options.previous_response_id,
        None,
        options.request_cwd,
        None,
        None,
        Some("gpt".to_string()),
        Some("medium".to_string()),
        true,
        options.include_prompt_cache_retention,
        options.output_format,
    );

    assert_eq!(
        payload.get("prompt_cache_key"),
        Some(&Value::String("session_1".to_string()))
    );
    assert_eq!(
        payload.get("prompt_cache_retention"),
        Some(&Value::String("24h".to_string()))
    );
    assert_eq!(
        payload.get("previous_response_id"),
        Some(&Value::String("resp_1".to_string()))
    );
    assert_eq!(
        payload.get("cwd"),
        Some(&Value::String("/workspace".to_string()))
    );
}

#[test]
fn ai_read_timeout_defaults_to_five_minutes_and_accepts_valid_override() {
    assert_eq!(parse_timeout_seconds(None, 300), 300);
    assert_eq!(parse_timeout_seconds(Some("450"), 300), 450);
    assert_eq!(parse_timeout_seconds(Some("0"), 300), 300);
    assert_eq!(parse_timeout_seconds(Some("invalid"), 300), 300);
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
        None,
        None,
        None,
        Some("openai".to_string()),
        None,
        true,
        false,
        None,
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
        None,
        None,
        None,
        Some("openai_compatible".to_string()),
        Some("xhigh".to_string()),
        true,
        false,
        None,
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
        None,
        None,
        None,
        Some("openai_compatible".to_string()),
        Some("high".to_string()),
        true,
        false,
        None,
    );

    assert_eq!(
        payload.pointer("/reasoning/effort"),
        Some(&Value::String("high".to_string()))
    );
    assert!(payload.pointer("/reasoning/summary").is_none());
}

#[test]
fn custom_openai_base_url_uses_compatible_provider() {
    assert_eq!(
        effective_provider_for_request(
            "https://gateway.example.test/v1",
            Some("openai".to_string()),
        )
        .as_deref(),
        Some("openai_compatible")
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
            false,
            "test-model".to_string(),
            None,
            None,
            None,
            None,
            StreamCallbacks::default(),
            Some("openai_compatible".to_string()),
            None,
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
