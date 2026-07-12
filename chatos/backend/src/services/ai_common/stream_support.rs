// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

#[cfg(test)]
use std::collections::HashSet;
#[cfg(test)]
use std::future::Future;
#[cfg(test)]
use std::sync::Arc;

#[cfg(test)]
use futures::{Stream, StreamExt};
#[cfg(test)]
use serde_json::json;
use serde_json::Value;
#[cfg(test)]
use tokio_util::sync::CancellationToken;

use crate::core::mcp_tools::ToolResult;
#[cfg(test)]
use crate::core::mcp_tools::ToolResultCallback;
#[cfg(test)]
use crate::core::messages::text_has_content;
#[cfg(test)]
use crate::core::tool_call::{extract_tool_call_id, extract_tool_call_name};
#[cfg(test)]
use crate::services::ai_client_common::AiClientCallbacks;
#[cfg(test)]
use crate::utils::abort_registry;

#[cfg(test)]
pub(crate) struct ToolExecutionOutcome {
    pub persisted_results: Vec<ToolResult>,
}

#[cfg(test)]
#[derive(Clone, Default)]
pub(crate) struct AiStreamCallbacks {
    pub on_chunk: Option<Arc<dyn Fn(String) + Send + Sync>>,
    pub on_thinking: Option<Arc<dyn Fn(String) + Send + Sync>>,
}

#[cfg(test)]
#[derive(Default)]
struct Utf8ChunkDecoder {
    pending: Vec<u8>,
}

#[cfg(test)]
impl Utf8ChunkDecoder {
    fn push(&mut self, bytes: &[u8]) -> String {
        self.pending.extend_from_slice(bytes);
        let mut out = String::new();

        loop {
            match std::str::from_utf8(self.pending.as_slice()) {
                Ok(text) => {
                    out.push_str(text);
                    self.pending.clear();
                    break;
                }
                Err(err) => {
                    let valid_up_to = err.valid_up_to();
                    if valid_up_to > 0 {
                        let valid =
                            std::str::from_utf8(&self.pending[..valid_up_to]).unwrap_or_default();
                        out.push_str(valid);
                        self.pending.drain(..valid_up_to);
                        continue;
                    }

                    if let Some(error_len) = err.error_len() {
                        out.push('\u{FFFD}');
                        self.pending.drain(..error_len);
                        continue;
                    }

                    break;
                }
            }
        }

        out
    }

    fn finish(&mut self) -> String {
        if self.pending.is_empty() {
            return String::new();
        }
        String::from_utf8_lossy(&std::mem::take(&mut self.pending)).to_string()
    }
}

#[cfg(test)]
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

#[cfg(test)]
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
    let mut decoder = Utf8ChunkDecoder::default();

    let mut process_chunk = |chunk: Result<bytes::Bytes, E>| -> Result<(), String> {
        let bytes = chunk.map_err(|err| err.to_string())?;
        let text = decoder.push(bytes.as_ref());
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

    let tail_text = decoder.finish();
    if !tail_text.is_empty() {
        buffer.push_str(&tail_text);
    }
    flush_stream_tail_events(&mut buffer, &mut on_event);

    Ok(())
}

#[cfg(test)]
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

#[cfg(test)]
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
    if let Some(structured_result) = result.result.clone() {
        map.insert("structured_result".to_string(), structured_result);
    }
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

#[cfg(test)]
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

#[cfg(test)]
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
        let id = extract_tool_call_id(tool_call).unwrap_or("").to_string();
        if id.is_empty() || present.contains(&id) {
            continue;
        }

        let name = extract_tool_call_name(tool_call)
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
            result: None,
        });
    }

    results
}

#[cfg(test)]
pub(crate) fn aborted_tool_results_if_needed(
    session_id: Option<&str>,
    persist_tool_messages: bool,
    tool_calls: &[Value],
    existing: Option<&[ToolResult]>,
) -> Option<Vec<ToolResult>> {
    let session_id = session_id
        .map(str::trim)
        .filter(|value| !value.is_empty())?;
    if !persist_tool_messages || !abort_registry::is_aborted(session_id) {
        return None;
    }
    Some(build_aborted_tool_results(tool_calls, existing))
}

#[cfg(test)]
pub(crate) fn build_tools_end_payload(tool_results: &[ToolResult]) -> Value {
    json!({
        "tool_results": tool_results,
    })
}

#[cfg(test)]
pub(crate) fn emit_stream_callbacks(
    callbacks: &AiStreamCallbacks,
    chunk: Option<String>,
    thinking: Option<String>,
) {
    if let Some(chunk) = chunk {
        if let Some(cb) = &callbacks.on_chunk {
            cb(chunk);
        }
    }

    if let Some(thinking) = thinking {
        if let Some(cb) = &callbacks.on_thinking {
            cb(thinking);
        }
    }
}

#[cfg(test)]
pub(crate) fn parsed_stream_response_is_empty(
    parsed_event_count: usize,
    content: &str,
    reasoning: &str,
    has_auxiliary_payload: bool,
) -> bool {
    parsed_event_count == 0
        && !text_has_content(content)
        && !text_has_content(reasoning)
        && !has_auxiliary_payload
}

#[cfg(test)]
pub(crate) async fn execute_tool_lifecycle<Exec, ExecFut, Finalize, Persist, PersistFut>(
    requested_tool_calls: &[Value],
    display_tool_calls: Value,
    session_id: Option<&str>,
    persist_tool_messages: bool,
    callbacks: &AiClientCallbacks,
    execute: Exec,
    finalize_results: Finalize,
    persist: Persist,
) -> Result<ToolExecutionOutcome, String>
where
    Exec: FnOnce(Option<ToolResultCallback>) -> ExecFut,
    ExecFut: Future<Output = Vec<ToolResult>>,
    Finalize: FnOnce(&[ToolResult]) -> Vec<ToolResult>,
    Persist: Fn(Vec<ToolResult>) -> PersistFut,
    PersistFut: Future<Output = ()>,
{
    if let Some(cb) = &callbacks.on_tools_start {
        cb(display_tool_calls);
    }

    if let Some(aborted_results) = aborted_tool_results_if_needed(
        session_id,
        persist_tool_messages,
        requested_tool_calls,
        None,
    ) {
        persist(aborted_results).await;
        return Err("aborted".to_string());
    }

    let on_tools_stream_cb = build_tool_stream_callback(
        callbacks.on_tools_stream.clone(),
        session_id.map(str::to_string),
    );
    let tool_results = execute(on_tools_stream_cb).await;
    let persisted_results = finalize_results(tool_results.as_slice());

    if let Some(aborted_results) = aborted_tool_results_if_needed(
        session_id,
        persist_tool_messages,
        requested_tool_calls,
        Some(persisted_results.as_slice()),
    ) {
        persist(aborted_results).await;
        return Err("aborted".to_string());
    }

    if let Some(cb) = &callbacks.on_tools_end {
        cb(build_tools_end_payload(tool_results.as_slice()));
    }

    if persist_tool_messages {
        persist(persisted_results.clone()).await;
    }

    Ok(ToolExecutionOutcome { persisted_results })
}
