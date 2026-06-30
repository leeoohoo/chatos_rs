use std::sync::{Arc, Mutex};

use serde_json::{json, Value};

use super::{
    build_chat_completions_request_payload, build_responses_request_payload,
    effective_provider_for_request, emit_finalized_stream_callbacks,
    response_items_to_chat_messages, validate_request_payload_size, AiRequestOptions,
    StreamCallbacks,
};
use crate::stream_parse::FinalizedStreamState;

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
fn responses_payload_supports_prompt_cache_and_cwd() {
    let options = AiRequestOptions {
        prompt_cache_key: Some("session_1".to_string()),
        request_cwd: Some("/workspace".to_string()),
        include_prompt_cache_retention: true,
        request_body_limit_bytes: None,
        abort_token: None,
        force_identity_encoding: false,
    };
    let payload = build_responses_request_payload(
        json!([]),
        "gpt-4.1".to_string(),
        Some("system".to_string()),
        options.prompt_cache_key,
        None,
        options.request_cwd,
        None,
        None,
        Some("gpt".to_string()),
        Some("medium".to_string()),
        true,
        options.include_prompt_cache_retention,
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
        payload.get("cwd"),
        Some(&Value::String("/workspace".to_string()))
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
        Some("openai_compatible".to_string()),
        Some("xhigh".to_string()),
        true,
        false,
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
        Some("openai_compatible".to_string()),
        Some("high".to_string()),
        true,
        false,
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
