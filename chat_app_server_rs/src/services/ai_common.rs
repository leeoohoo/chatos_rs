use std::collections::HashSet;
use std::future::Future;
use std::sync::Arc;

use futures::{Stream, StreamExt};
use serde_json::{json, Value};
use tokio_util::sync::CancellationToken;

use crate::core::mcp_tools::{ToolResult, ToolResultCallback};
use crate::utils::abort_registry;
use crate::utils::attachments::{self, Attachment};

pub(crate) fn normalize_turn_id(turn_id: Option<&str>) -> Option<String> {
    turn_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.to_string())
}

pub(crate) fn build_user_message_metadata(
    attachments_list: &[Attachment],
    turn_id: Option<&str>,
) -> Option<Value> {
    let sanitized = attachments::sanitize_attachments_for_db(attachments_list);

    if sanitized.is_empty() && turn_id.is_none() {
        return None;
    }

    let mut map = serde_json::Map::new();
    if !sanitized.is_empty() {
        map.insert("attachments".to_string(), Value::Array(sanitized));
    }
    if let Some(turn) = turn_id {
        map.insert(
            "conversation_turn_id".to_string(),
            Value::String(turn.to_string()),
        );
    }

    Some(Value::Object(map))
}

pub(crate) async fn build_user_content_parts(
    model: &str,
    user_message: &str,
    attachments_list: &[Attachment],
    supports_images: Option<bool>,
) -> Value {
    let content_parts =
        attachments::build_content_parts_async(user_message, attachments_list).await;
    attachments::adapt_parts_for_model(model, &content_parts, supports_images)
}

pub(crate) fn normalize_reasoning_effort(
    provider: Option<&str>,
    level: Option<&str>,
) -> Option<String> {
    let provider = provider.unwrap_or("gpt");
    crate::utils::model_config::normalize_thinking_level(provider, level).unwrap_or_default()
}

pub(crate) fn truncate_log(value: &str, max_len: usize) -> String {
    if value.len() <= max_len {
        return value.to_string();
    }

    let mut out = value[..max_len].to_string();
    out.push_str("...[truncated]");
    out
}

pub(crate) fn completion_failed_error(
    finish_reason: Option<&str>,
    content: &str,
    reasoning: Option<&str>,
    provider_error: Option<&Value>,
) -> Option<String> {
    let reason = finish_reason
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("unknown");
    let normalized = reason.to_ascii_lowercase();
    if normalized != "failed" && normalized != "error" {
        return None;
    }

    let mut segments = vec![format!("finish_reason={}", reason)];

    if let Some(error_preview) = provider_error
        .and_then(build_provider_error_preview)
        .filter(|value| !value.trim().is_empty())
    {
        segments.push(format!("provider_error={}", error_preview));
    }

    if !content.trim().is_empty() {
        segments.push(format!("content_preview={}", truncate_log(content, 300)));
        return Some(format!("ai response failed: {}", segments.join("; ")));
    }

    if let Some(reasoning) = reasoning {
        if !reasoning.trim().is_empty() {
            segments.push(format!(
                "reasoning_preview={}",
                truncate_log(reasoning, 300)
            ));
            return Some(format!("ai response failed: {}", segments.join("; ")));
        }
    }

    Some(format!("ai response failed: {}", segments.join("; ")))
}

fn build_provider_error_preview(provider_error: &Value) -> Option<String> {
    if provider_error.is_null() {
        return None;
    }

    if let Some(object) = provider_error.as_object() {
        let mut parts = Vec::new();
        if let Some(code) = object.get("code").and_then(|v| v.as_str()) {
            if !code.trim().is_empty() {
                parts.push(format!("code={}", code.trim()));
            }
        }
        if let Some(kind) = object.get("type").and_then(|v| v.as_str()) {
            if !kind.trim().is_empty() {
                parts.push(format!("type={}", kind.trim()));
            }
        }
        if let Some(message) = object.get("message").and_then(|v| v.as_str()) {
            if !message.trim().is_empty() {
                parts.push(format!("message={}", truncate_log(message.trim(), 300)));
            }
        }
        if let Some(param) = object.get("param").and_then(|v| v.as_str()) {
            if !param.trim().is_empty() {
                parts.push(format!("param={}", param.trim()));
            }
        }

        if !parts.is_empty() {
            return Some(parts.join(", "));
        }
    }

    let raw = provider_error.to_string();
    if raw.trim().is_empty() {
        None
    } else {
        Some(truncate_log(raw.trim(), 300))
    }
}

pub(crate) fn build_assistant_message_metadata(
    tool_calls: Option<&Value>,
    response_id: Option<&str>,
) -> Option<Value> {
    let mut map = serde_json::Map::new();

    if let Some(id) = response_id.map(str::trim).filter(|value| !value.is_empty()) {
        map.insert("response_id".to_string(), Value::String(id.to_string()));
    }
    if let Some(tool_calls) = tool_calls {
        map.insert("toolCalls".to_string(), tool_calls.clone());
    }

    if map.is_empty() {
        None
    } else {
        Some(Value::Object(map))
    }
}

pub(crate) fn drain_sse_json_events(buffer: &mut String) -> Vec<Value> {
    let mut events = Vec::new();

    while let Some(idx) = buffer.find("\n\n") {
        let packet = buffer[..idx].to_string();
        *buffer = buffer[idx + 2..].to_string();

        for line in packet.lines() {
            let line = line.trim();
            if !line.starts_with("data:") {
                continue;
            }

            let data = line.trim_start_matches("data:").trim();
            if data == "[DONE]" {
                break;
            }
            if data.is_empty() {
                continue;
            }

            if let Ok(value) = serde_json::from_str::<Value>(data) {
                events.push(value);
            }
        }
    }

    events
}

pub(crate) async fn await_with_optional_abort<F, T, E>(
    future: F,
    token: Option<CancellationToken>,
) -> Result<T, String>
where
    F: Future<Output = Result<T, E>>,
    E: ToString,
{
    if let Some(token) = token {
        tokio::select! {
            _ = token.cancelled() => Err("aborted".to_string()),
            value = future => value.map_err(|err| err.to_string()),
        }
    } else {
        future.await.map_err(|err| err.to_string())
    }
}

pub(crate) async fn consume_sse_stream<S, E, F>(
    mut stream: S,
    token: Option<CancellationToken>,
    mut on_event: F,
) -> Result<(), String>
where
    S: Stream<Item = Result<bytes::Bytes, E>> + Unpin,
    E: ToString,
    F: FnMut(Value),
{
    let mut buffer = String::new();

    while let Some(chunk) = stream.next().await {
        if let Some(token) = token.as_ref() {
            if token.is_cancelled() {
                return Err("aborted".to_string());
            }
        }

        let bytes = chunk.map_err(|err| err.to_string())?;
        let text = String::from_utf8_lossy(&bytes).to_string();
        buffer.push_str(&text);

        for event in drain_sse_json_events(&mut buffer) {
            on_event(event);
        }
    }

    flush_stream_tail_events(&mut buffer, &mut on_event);

    Ok(())
}

fn flush_stream_tail_events<F>(buffer: &mut String, on_event: &mut F)
where
    F: FnMut(Value),
{
    if buffer.trim().is_empty() {
        return;
    }

    if buffer.contains("data:") {
        if !buffer.ends_with("\n\n") {
            buffer.push_str("\n\n");
        }
        for event in drain_sse_json_events(buffer) {
            on_event(event);
        }
    }

    let tail = buffer.trim();
    if tail.is_empty() {
        return;
    }

    if let Ok(value) = serde_json::from_str::<Value>(tail) {
        emit_json_value(value, on_event);
        buffer.clear();
    }
}

fn emit_json_value<F>(value: Value, on_event: &mut F)
where
    F: FnMut(Value),
{
    if let Some(array) = value.as_array() {
        for item in array {
            if item.is_object() {
                on_event(item.clone());
            }
        }
        return;
    }

    if value.is_object() {
        on_event(value);
    }
}

pub(crate) fn build_tool_result_metadata(result: &ToolResult) -> Value {
    json!({
        "toolName": result.name,
        "success": result.success,
        "isError": result.is_error
    })
}

pub(crate) fn build_tool_stream_callback(
    callback: Option<Arc<dyn Fn(Value) + Send + Sync>>,
    session_id: Option<String>,
) -> Option<ToolResultCallback> {
    callback.map(|cb| {
        let sid = session_id.clone();
        Arc::new(move |result: &ToolResult| {
            if let Some(ref sid) = sid {
                if abort_registry::is_aborted(sid) {
                    return;
                }
            }

            cb(serde_json::to_value(result).unwrap_or(json!({})));
        }) as ToolResultCallback
    })
}

pub(crate) fn build_aborted_tool_results(
    tool_calls: &[Value],
    existing: Option<&[ToolResult]>,
) -> Vec<ToolResult> {
    let mut results = existing.map(|items| items.to_vec()).unwrap_or_default();
    let mut present: HashSet<String> = results
        .iter()
        .map(|item| item.tool_call_id.clone())
        .collect();

    for tool_call in tool_calls {
        let id = tool_call
            .get("id")
            .and_then(|value| value.as_str())
            .unwrap_or("")
            .to_string();
        if id.is_empty() || present.contains(&id) {
            continue;
        }

        let name = tool_call
            .get("function")
            .and_then(|function| function.get("name"))
            .and_then(|value| value.as_str())
            .or_else(|| tool_call.get("name").and_then(|value| value.as_str()))
            .unwrap_or("tool")
            .to_string();

        present.insert(id.clone());
        results.push(ToolResult {
            tool_call_id: id,
            name,
            success: false,
            is_error: true,
            is_stream: false,
            content: "aborted".to_string(),
        });
    }

    results
}

#[cfg(test)]
mod tests {
    use super::*;

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

        let err = completion_failed_error(Some("error"), "body", None, None)
            .expect("should return error");
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
        assert!(build_assistant_message_metadata(None, None).is_none());
        assert!(build_assistant_message_metadata(None, Some("   ")).is_none());
    }

    #[test]
    fn build_assistant_message_metadata_keeps_response_id_and_tool_calls() {
        let tool_calls = serde_json::json!([{"id": "call_1"}]);
        let metadata = build_assistant_message_metadata(Some(&tool_calls), Some("resp_123"));

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
    }

    #[test]
    fn build_tool_result_metadata_keeps_tool_flags() {
        let result = ToolResult {
            tool_call_id: "call_1".to_string(),
            name: "mcp.query".to_string(),
            success: true,
            is_error: false,
            is_stream: false,
            content: "ok".to_string(),
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
            content: "done".to_string(),
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
            content: "ok".to_string(),
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
            content: "ok".to_string(),
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
}
