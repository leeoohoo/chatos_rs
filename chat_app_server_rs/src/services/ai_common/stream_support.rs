use std::collections::HashSet;
use std::sync::Arc;

use futures::{Stream, StreamExt};
use serde_json::{json, Value};
use tokio_util::sync::CancellationToken;

use crate::core::mcp_tools::{ToolResult, ToolResultCallback};
use crate::utils::abort_registry;

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

    let mut process_chunk = |chunk: Result<bytes::Bytes, E>| -> Result<(), String> {
        let bytes = chunk.map_err(|err| err.to_string())?;
        let text = String::from_utf8_lossy(&bytes).to_string();
        buffer.push_str(&text);

        for event in drain_sse_json_events(&mut buffer) {
            on_event(event);
        }

        Ok(())
    };

    if let Some(token) = token {
        loop {
            tokio::select! {
                _ = token.cancelled() => {
                    return Err("aborted".to_string());
                }
                next = stream.next() => {
                    match next {
                        Some(chunk) => process_chunk(chunk)?,
                        None => break,
                    }
                }
            }
        }
    } else {
        while let Some(chunk) = stream.next().await {
            process_chunk(chunk)?;
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
    let mut map = serde_json::Map::new();
    map.insert("toolName".to_string(), Value::String(result.name.clone()));
    map.insert("success".to_string(), Value::Bool(result.success));
    map.insert("isError".to_string(), Value::Bool(result.is_error));
    if let Some(turn_id) = result
        .conversation_turn_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        map.insert(
            "conversation_turn_id".to_string(),
            Value::String(turn_id.to_string()),
        );
    }
    Value::Object(map)
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
            conversation_turn_id: None,
            content: "aborted".to_string(),
        });
    }

    results
}
