use serde_json::json;

use crate::services::ai_common::{
    is_non_terminal_response_status, should_persist_assistant_message,
    validate_request_payload_size,
};

use super::{
    build_chat_completions_request_payload, build_request_payload,
    is_prompt_cache_retention_unsupported_error, read_timeout_env_ms,
    should_retry_without_prompt_cache_retention, AiResponse, REQUEST_BODY_LIMIT_ENV,
};

#[test]
fn payload_precheck_accepts_small_payload() {
    let payload = json!({
        "model": "gpt-4o",
        "input": [{"role": "user", "content": [{"type":"input_text","text":"hello"}]}]
    });
    assert!(validate_request_payload_size(&payload, REQUEST_BODY_LIMIT_ENV).is_ok());
}

#[test]
fn payload_precheck_rejects_oversized_payload() {
    let payload = json!({
        "model": "gpt-4o",
        "input": [{"role": "user", "content": [{"type":"input_text","text":"a".repeat(1_700_000)}]}]
    });
    let err =
        validate_request_payload_size(&payload, REQUEST_BODY_LIMIT_ENV).expect_err("should reject");
    assert!(err.contains("request body too large"));
}

#[test]
fn build_request_payload_includes_request_cwd_when_present() {
    let payload = build_request_payload(
        json!([{"role":"user","content":[{"type":"input_text","text":"hello"}]}]),
        "gpt-5.3-codex".to_string(),
        Some("system".to_string()),
        Some("session-123".to_string()),
        None,
        Some("/tmp/worktree".to_string()),
        Some(0.2),
        Some(256),
        Some("gpt".to_string()),
        Some("medium".to_string()),
        true,
        true,
    );

    assert_eq!(
        payload.get("cwd").and_then(|value| value.as_str()),
        Some("/tmp/worktree")
    );
    assert_eq!(
        payload
            .get("prompt_cache_key")
            .and_then(|value| value.as_str()),
        Some("session-123")
    );
    assert_eq!(
        payload
            .get("prompt_cache_retention")
            .and_then(|value| value.as_str()),
        Some("24h")
    );
    assert_eq!(
        payload.get("stream").and_then(|value| value.as_bool()),
        Some(true)
    );
}

#[test]
fn build_chat_completions_payload_converts_responses_input() {
    let payload = build_chat_completions_request_payload(
        json!([
            {
                "type": "message",
                "role": "user",
                "content": [{"type":"input_text","text":"hello"}]
            },
            {
                "type": "function_call",
                "call_id": "call_1",
                "name": "demo.search",
                "arguments": "{\"q\":\"rust\"}"
            },
            {
                "type": "function_call_output",
                "call_id": "call_1",
                "output": "done"
            }
        ]),
        "deepseek-chat".to_string(),
        Some("system prompt".to_string()),
        Some(vec![json!({
            "type": "function",
            "function": {
                "name": "demo.search",
                "parameters": {"type":"object"}
            }
        })]),
        Some(0.2),
        Some(512),
        Some("deepseek".to_string()),
        None,
        true,
    );

    assert_eq!(payload.get("input"), None);
    assert_eq!(
        payload.get("model").and_then(|value| value.as_str()),
        Some("deepseek-chat")
    );
    assert_eq!(
        payload.get("stream").and_then(|value| value.as_bool()),
        Some(true)
    );
    assert_eq!(
        payload.get("max_tokens").and_then(|value| value.as_i64()),
        Some(512)
    );
    let messages = payload
        .get("messages")
        .and_then(|value| value.as_array())
        .expect("messages array");
    assert_eq!(messages.len(), 4);
    assert_eq!(
        messages[0].get("role").and_then(|value| value.as_str()),
        Some("system")
    );
    assert_eq!(
        messages[1]
            .get("content")
            .and_then(|value| value.as_array())
            .and_then(|items| items.first())
            .and_then(|item| item.get("text"))
            .and_then(|value| value.as_str()),
        Some("hello")
    );
    assert_eq!(
        messages[2]
            .get("tool_calls")
            .and_then(|value| value.as_array())
            .and_then(|items| items.first())
            .and_then(|item| item.get("id"))
            .and_then(|value| value.as_str()),
        Some("call_1")
    );
    assert_eq!(
        messages[3]
            .get("tool_call_id")
            .and_then(|value| value.as_str()),
        Some("call_1")
    );
    assert_eq!(
        payload
            .get("tools")
            .and_then(|value| value.as_array())
            .and_then(|items| items.first())
            .and_then(|item| item.get("function"))
            .and_then(|value| value.get("name"))
            .and_then(|value| value.as_str()),
        Some("demo.search")
    );
    assert_eq!(
        payload
            .get("tools")
            .and_then(|value| value.as_array())
            .and_then(|items| items.first())
            .and_then(|item| item.get("function"))
            .and_then(|value| value.get("parameters"))
            .and_then(|value| value.get("type"))
            .and_then(|value| value.as_str()),
        Some("object")
    );
}

#[test]
fn build_chat_completions_payload_wraps_flat_tool_schema_in_function_object() {
    let payload = build_chat_completions_request_payload(
        json!([{
            "type": "message",
            "role": "user",
            "content": [{"type":"input_text","text":"hello"}]
        }]),
        "deepseek-chat".to_string(),
        None,
        Some(vec![json!({
            "type": "function",
            "name": "builtin_search",
            "description": "Search the workspace",
            "parameters": {"type":"object","properties":{"q":{"type":"string"}}}
        })]),
        None,
        None,
        Some("deepseek".to_string()),
        None,
        true,
    );

    let first_tool = payload
        .get("tools")
        .and_then(|value| value.as_array())
        .and_then(|items| items.first())
        .expect("tool entry");

    assert_eq!(
        first_tool.get("type").and_then(|value| value.as_str()),
        Some("function")
    );
    assert_eq!(
        first_tool
            .get("function")
            .and_then(|value| value.get("name"))
            .and_then(|value| value.as_str()),
        Some("builtin_search")
    );
    assert_eq!(
        first_tool
            .get("function")
            .and_then(|value| value.get("description"))
            .and_then(|value| value.as_str()),
        Some("Search the workspace")
    );
}

#[test]
fn build_chat_completions_payload_preserves_assistant_reasoning_content() {
    let payload = build_chat_completions_request_payload(
        json!([
            {
                "type": "message",
                "role": "assistant",
                "content": [
                    {"type":"reasoning","text":"step-1"},
                    {"type":"output_text","text":"tool call incoming"}
                ]
            },
            {
                "type": "function_call",
                "call_id": "call_1",
                "name": "demo.search",
                "arguments": "{\"q\":\"rust\"}"
            },
            {
                "type": "function_call_output",
                "call_id": "call_1",
                "output": "done"
            }
        ]),
        "deepseek-chat".to_string(),
        None,
        None,
        None,
        None,
        Some("deepseek".to_string()),
        Some("high".to_string()),
        true,
    );

    let messages = payload
        .get("messages")
        .and_then(|value| value.as_array())
        .expect("messages array");

    assert_eq!(
        messages[0]
            .get("reasoning_content")
            .and_then(|value| value.as_str()),
        Some("step-1")
    );
    assert_eq!(
        messages[0].get("role").and_then(|value| value.as_str()),
        Some("assistant")
    );
    assert_eq!(
        messages[1].get("role").and_then(|value| value.as_str()),
        Some("tool")
    );
    assert_eq!(
        messages[1]
            .get("tool_call_id")
            .and_then(|value| value.as_str()),
        Some("call_1")
    );
}

#[test]
fn build_chat_completions_payload_omits_temperature_for_deepseek_thinking() {
    let payload = build_chat_completions_request_payload(
        json!([{
            "type": "message",
            "role": "user",
            "content": [{"type":"input_text","text":"hello"}]
        }]),
        "deepseek-reasoner".to_string(),
        None,
        None,
        Some(0.7),
        None,
        Some("deepseek".to_string()),
        Some("max".to_string()),
        true,
    );

    assert_eq!(payload.get("temperature"), None);
    assert_eq!(
        payload
            .get("reasoning_effort")
            .and_then(|value| value.as_str()),
        Some("max")
    );
    assert_eq!(
        payload
            .get("thinking")
            .and_then(|value| value.get("type"))
            .and_then(|value| value.as_str()),
        Some("enabled")
    );
}

#[test]
fn build_chat_completions_payload_groups_tool_calls_before_tool_outputs() {
    let payload = build_chat_completions_request_payload(
        json!([
            {
                "type": "message",
                "role": "assistant",
                "content": [{"type":"output_text","text":"calling tools"}]
            },
            {
                "type": "function_call",
                "call_id": "call_1",
                "name": "demo.one",
                "arguments": "{}"
            },
            {
                "type": "function_call",
                "call_id": "call_2",
                "name": "demo.two",
                "arguments": "{}"
            },
            {
                "type": "function_call_output",
                "call_id": "call_1",
                "output": "out-1"
            },
            {
                "type": "function_call_output",
                "call_id": "call_2",
                "output": "out-2"
            }
        ]),
        "deepseek-chat".to_string(),
        None,
        None,
        None,
        None,
        Some("deepseek".to_string()),
        None,
        true,
    );

    let messages = payload
        .get("messages")
        .and_then(|value| value.as_array())
        .expect("messages array");

    assert_eq!(messages.len(), 3);
    assert_eq!(
        messages[0]
            .get("tool_calls")
            .and_then(|value| value.as_array())
            .map(|items| items.len()),
        Some(2)
    );
    assert_eq!(
        messages[1].get("role").and_then(|value| value.as_str()),
        Some("tool")
    );
    assert_eq!(
        messages[2].get("role").and_then(|value| value.as_str()),
        Some("tool")
    );
}

#[test]
fn retries_when_provider_reports_prompt_cache_retention_not_supported() {
    let attempt: Result<AiResponse, String> =
        Err("status 400 Bad Request: Unsupported parameter: prompt_cache_retention".to_string());
    let payload = serde_json::json!({
        "prompt_cache_retention": "24h",
    });

    assert!(should_retry_without_prompt_cache_retention(
        &attempt, &payload
    ));
}

#[test]
fn retries_when_provider_reports_unknown_parameter_wording() {
    assert!(is_prompt_cache_retention_unsupported_error(
        "status 400: unknown parameter `prompt_cache_retention`",
    ));
    assert!(is_prompt_cache_retention_unsupported_error(
        "status 400: prompt_cache_retention is not supported by upstream",
    ));
    assert!(!is_prompt_cache_retention_unsupported_error(
        "status 500: upstream timeout",
    ));
}

#[test]
fn read_timeout_env_ms_clamps_values() {
    std::env::remove_var("AI_AGENT_TEST_TIMEOUT");
    assert_eq!(read_timeout_env_ms("AI_AGENT_TEST_TIMEOUT", 12_345), 12_345);

    std::env::set_var("AI_AGENT_TEST_TIMEOUT", "10");
    assert_eq!(read_timeout_env_ms("AI_AGENT_TEST_TIMEOUT", 12_345), 1_000);

    std::env::set_var("AI_AGENT_TEST_TIMEOUT", "9999999");
    assert_eq!(
        read_timeout_env_ms("AI_AGENT_TEST_TIMEOUT", 12_345),
        600_000
    );

    std::env::set_var("AI_AGENT_TEST_TIMEOUT", "abc");
    assert_eq!(read_timeout_env_ms("AI_AGENT_TEST_TIMEOUT", 12_345), 12_345);

    std::env::remove_var("AI_AGENT_TEST_TIMEOUT");
}

#[test]
fn build_request_payload_skips_blank_prompt_cache_key() {
    let payload = build_request_payload(
        json!([{"role":"user","content":[{"type":"input_text","text":"hello"}]}]),
        "gpt-5.3-codex".to_string(),
        Some("system".to_string()),
        Some("   ".to_string()),
        None,
        None,
        None,
        None,
        Some("gpt".to_string()),
        Some("medium".to_string()),
        true,
        true,
    );

    assert!(payload.get("prompt_cache_key").is_none());
    assert!(payload.get("prompt_cache_retention").is_none());
}

#[test]
fn build_request_payload_never_includes_prev_id() {
    let payload = build_request_payload(
        json!([{"role":"user","content":[{"type":"input_text","text":"hello"}]}]),
        "gpt-5.3-codex".to_string(),
        Some("system".to_string()),
        None,
        None,
        None,
        None,
        None,
        Some("gpt".to_string()),
        Some("medium".to_string()),
        true,
        true,
    );

    assert!(payload.get("prev_id").is_none());
}

#[test]
fn marks_non_terminal_statuses() {
    assert!(is_non_terminal_response_status(Some("in_progress")));
    assert!(is_non_terminal_response_status(Some("queued")));
    assert!(!is_non_terminal_response_status(Some("completed")));
    assert!(!is_non_terminal_response_status(None));
}

#[test]
fn skips_persist_for_non_terminal_empty_response() {
    assert!(!should_persist_assistant_message(
        "",
        None,
        None,
        Some("in_progress"),
    ));
    assert!(should_persist_assistant_message(
        "hello",
        None,
        None,
        Some("in_progress"),
    ));
    assert!(should_persist_assistant_message(
        "",
        Some("thought"),
        None,
        Some("in_progress"),
    ));
}
