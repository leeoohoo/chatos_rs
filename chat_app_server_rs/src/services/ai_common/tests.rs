use std::sync::Arc;

use serde_json::{json, Value};
use tokio_util::sync::CancellationToken;

use super::*;
use crate::core::mcp_tools::ToolResult;
use crate::utils::abort_registry;

#[test]
fn normalize_turn_id_trims_and_filters_empty_values() {
    assert_eq!(
        normalize_turn_id(Some("  turn-1 ")),
        Some("turn-1".to_string())
    );
    assert_eq!(normalize_turn_id(Some("   ")), None);
    assert_eq!(normalize_turn_id(None), None);
}

#[test]
fn truncate_log_adds_suffix_when_exceeding_limit() {
    let value = truncate_log("abcdefgh", 4);
    assert_eq!(value, "abcd...[truncated]");
    assert_eq!(truncate_log("abc", 4), "abc");
}

#[test]
fn completion_failed_error_uses_finish_reason_and_preview() {
    let err = completion_failed_error(Some("failed"), "", Some("detailed reasoning"), None)
        .expect("should return error");
    assert!(err.contains("finish_reason=failed"));
    assert!(err.contains("reasoning_preview=detailed reasoning"));

    let err =
        completion_failed_error(Some("error"), "body", None, None).expect("should return error");
    assert!(err.contains("content_preview=body"));

    let provider_error = json!({
        "code": "context_length_exceeded",
        "type": "invalid_request_error",
        "message": "too long",
        "param": "input"
    });
    let err = completion_failed_error(Some("failed"), "", None, Some(&provider_error))
        .expect("should include provider error");
    assert!(err.contains("provider_error=code=context_length_exceeded"));
    assert!(err.contains("type=invalid_request_error"));
    assert!(err.contains("message=too long"));

    assert!(completion_failed_error(Some("stop"), "", None, None).is_none());
}

#[test]
fn build_assistant_message_metadata_skips_empty_fields() {
    assert!(build_assistant_message_metadata(None, None, None, None).is_none());
    assert!(build_assistant_message_metadata(None, Some("   "), None, None).is_none());
}

#[test]
fn build_assistant_message_metadata_keeps_response_id_and_tool_calls() {
    let tool_calls = serde_json::json!([{"id": "call_1"}]);
    let metadata = build_assistant_message_metadata(
        Some(&tool_calls),
        Some("resp_123"),
        Some("turn_123"),
        Some("completed"),
    );

    assert_eq!(
        metadata
            .as_ref()
            .and_then(|value| value.get("response_id"))
            .and_then(|value| value.as_str()),
        Some("resp_123")
    );
    assert_eq!(
        metadata
            .as_ref()
            .and_then(|value| value.get("toolCalls"))
            .cloned(),
        Some(tool_calls)
    );
    assert_eq!(
        metadata
            .as_ref()
            .and_then(|value| value.get("conversation_turn_id"))
            .and_then(|value| value.as_str()),
        Some("turn_123")
    );
    assert_eq!(
        metadata
            .as_ref()
            .and_then(|value| value.get("response_status"))
            .and_then(|value| value.as_str()),
        Some("completed")
    );
}

#[test]
fn build_tool_result_metadata_keeps_tool_flags() {
    let result = ToolResult {
        tool_call_id: "call_1".to_string(),
        name: "mcp.query".to_string(),
        success: true,
        is_error: false,
        is_stream: false,
        conversation_turn_id: Some("turn_abc".to_string()),
        content: "ok".to_string(),
        result: Some(serde_json::json!({"answer": 42})),
    };

    let metadata = build_tool_result_metadata(&result);

    assert_eq!(
        metadata.get("toolName").and_then(|value| value.as_str()),
        Some("mcp.query")
    );
    assert_eq!(
        metadata.get("success").and_then(|value| value.as_bool()),
        Some(true)
    );
    assert_eq!(
        metadata.get("isError").and_then(|value| value.as_bool()),
        Some(false)
    );
    assert_eq!(
        metadata
            .get("conversation_turn_id")
            .and_then(|value| value.as_str()),
        Some("turn_abc")
    );
    assert_eq!(
        metadata
            .get("structured_result")
            .and_then(|value| value.get("answer"))
            .and_then(|value| value.as_i64()),
        Some(42)
    );
}

#[test]
fn build_aborted_tool_results_only_adds_missing_calls() {
    let tool_calls = vec![
        serde_json::json!({"id": "call_existing", "function": {"name": "tool.a"}}),
        serde_json::json!({"id": "call_missing", "function": {"name": "tool.b"}}),
        serde_json::json!({"id": "", "function": {"name": "tool.c"}}),
    ];

    let existing = vec![ToolResult {
        tool_call_id: "call_existing".to_string(),
        name: "tool.a".to_string(),
        success: true,
        is_error: false,
        is_stream: false,
        conversation_turn_id: None,
        content: "done".to_string(),
        result: None,
    }];

    let merged = build_aborted_tool_results(&tool_calls, Some(existing.as_slice()));

    assert_eq!(merged.len(), 2);
    assert!(merged
        .iter()
        .any(|item| item.tool_call_id == "call_existing" && item.success));
    assert!(merged
        .iter()
        .any(|item| item.tool_call_id == "call_missing" && !item.success && item.is_error));
}

#[test]
fn build_tool_stream_callback_emits_result_when_not_aborted() {
    let session_id = "ai_common_tool_stream_emit";
    abort_registry::clear(session_id);

    let captured = Arc::new(std::sync::Mutex::new(Vec::<Value>::new()));
    let on_stream = {
        let captured = captured.clone();
        Arc::new(move |value: Value| {
            captured.lock().expect("lock poisoned").push(value);
        }) as Arc<dyn Fn(Value) + Send + Sync>
    };

    let callback = build_tool_stream_callback(Some(on_stream), Some(session_id.to_string()))
        .expect("callback should be built");

    callback(&ToolResult {
        tool_call_id: "call_1".to_string(),
        name: "mcp.search".to_string(),
        success: true,
        is_error: false,
        is_stream: false,
        conversation_turn_id: None,
        content: "ok".to_string(),
        result: None,
    });

    let events = captured.lock().expect("lock poisoned");
    assert_eq!(events.len(), 1);
    assert_eq!(
        events[0]
            .get("tool_call_id")
            .and_then(|value| value.as_str()),
        Some("call_1")
    );

    abort_registry::clear(session_id);
}

#[test]
fn build_tool_stream_callback_skips_result_when_aborted() {
    let session_id = "ai_common_tool_stream_aborted";
    abort_registry::clear(session_id);
    abort_registry::abort(session_id);

    let captured = Arc::new(std::sync::Mutex::new(Vec::<Value>::new()));
    let on_stream = {
        let captured = captured.clone();
        Arc::new(move |value: Value| {
            captured.lock().expect("lock poisoned").push(value);
        }) as Arc<dyn Fn(Value) + Send + Sync>
    };

    let callback = build_tool_stream_callback(Some(on_stream), Some(session_id.to_string()))
        .expect("callback should be built");

    callback(&ToolResult {
        tool_call_id: "call_2".to_string(),
        name: "mcp.read".to_string(),
        success: true,
        is_error: false,
        is_stream: false,
        conversation_turn_id: None,
        content: "ok".to_string(),
        result: None,
    });

    assert!(captured.lock().expect("lock poisoned").is_empty());

    abort_registry::clear(session_id);
}

#[test]
fn drain_sse_json_events_parses_packets_and_keeps_incomplete_tail() {
    let mut buffer = concat!(
        "data: {\"type\":\"delta\",\"text\":\"hi\"}\n\n",
        "data: [DONE]\n\n",
        "data: {bad json}\n\n",
        "data: {\"type\":\"usage\",\"value\":1}\n\n",
        "data: {\"tail\":true}"
    )
    .to_string();

    let events = drain_sse_json_events(&mut buffer);

    assert_eq!(events.len(), 2);
    assert_eq!(
        events[0].get("type").and_then(|value| value.as_str()),
        Some("delta")
    );
    assert_eq!(
        events[1].get("type").and_then(|value| value.as_str()),
        Some("usage")
    );
    assert_eq!(buffer, "data: {\"tail\":true}");
}

#[tokio::test]
async fn consume_sse_stream_emits_events_and_ignores_done_lines() {
    use bytes::Bytes;
    use futures::stream;

    let chunks = vec![
        Ok::<Bytes, String>(Bytes::from("data: {\"type\":\"delta\",\"text\":\"a\"}\n\n")),
        Ok::<Bytes, String>(Bytes::from(
            "data: [DONE]\n\ndata: {\"type\":\"usage\",\"count\":1}\n\n",
        )),
    ];

    let mut events = Vec::new();
    consume_sse_stream(stream::iter(chunks), None, |event| {
        events.push(event);
    })
    .await
    .expect("stream parsing should succeed");

    assert_eq!(events.len(), 2);
    assert_eq!(
        events[0].get("type").and_then(|value| value.as_str()),
        Some("delta")
    );
    assert_eq!(
        events[1].get("type").and_then(|value| value.as_str()),
        Some("usage")
    );
}

#[tokio::test]
async fn consume_sse_stream_parses_trailing_plain_json_response() {
    use bytes::Bytes;
    use futures::stream;

    let chunks = vec![Ok::<Bytes, String>(Bytes::from(
        "{\"output_text\":\"summary text\",\"status\":\"completed\"}",
    ))];

    let mut events = Vec::new();
    consume_sse_stream(stream::iter(chunks), None, |event| {
        events.push(event);
    })
    .await
    .expect("stream parsing should succeed");

    assert_eq!(events.len(), 1);
    assert_eq!(
        events[0]
            .get("output_text")
            .and_then(|value| value.as_str()),
        Some("summary text")
    );
}

#[tokio::test]
async fn consume_sse_stream_returns_aborted_immediately_when_token_cancelled() {
    use futures::stream;
    use tokio::time::{sleep, timeout, Duration};

    let token = CancellationToken::new();
    let cancel_token = token.clone();
    tokio::spawn(async move {
        sleep(Duration::from_millis(20)).await;
        cancel_token.cancel();
    });

    let mut events = Vec::new();
    let result = timeout(
        Duration::from_millis(300),
        consume_sse_stream(
            stream::pending::<Result<bytes::Bytes, String>>(),
            Some(token),
            |event| events.push(event),
        ),
    )
    .await
    .expect("consume_sse_stream should not hang after cancellation");

    assert_eq!(result, Err("aborted".to_string()));
    assert!(events.is_empty());
}

#[tokio::test]
async fn await_with_optional_abort_returns_future_value_without_token() {
    let value = await_with_optional_abort(futures::future::ready(Ok::<i32, String>(7)), None)
        .await
        .expect("future should resolve");

    assert_eq!(value, 7);
}

#[tokio::test]
async fn await_with_optional_abort_returns_aborted_when_token_cancelled() {
    let token = CancellationToken::new();
    token.cancel();

    let result = await_with_optional_abort(
        futures::future::pending::<Result<i32, String>>(),
        Some(token),
    )
    .await;

    assert_eq!(result, Err("aborted".to_string()));
}
