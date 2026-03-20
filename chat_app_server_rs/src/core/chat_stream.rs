use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use serde_json::{json, Value};

use crate::services::v2::ai_client::AiClientCallbacks as V2AiClientCallbacks;
use crate::services::v3::ai_client::AiClientCallbacks as V3AiClientCallbacks;
use crate::utils::abort_registry;
use crate::utils::events::Events;
use crate::utils::sse::SseSender;

pub struct StreamCallbacksV2 {
    pub callbacks: V2AiClientCallbacks,
    pub chunk_sent: Arc<AtomicBool>,
    pub streamed_content: Arc<Mutex<String>>,
}

pub struct StreamCallbacksV3 {
    pub callbacks: V3AiClientCallbacks,
    pub chunk_sent: Arc<AtomicBool>,
    pub streamed_content: Arc<Mutex<String>>,
}

pub fn build_v2_callbacks(sender: &SseSender, session_id: &str) -> StreamCallbacksV2 {
    let sid = session_id.to_string();
    let chunk_sent = Arc::new(AtomicBool::new(false));
    let streamed_content = Arc::new(Mutex::new(String::new()));

    let sender_chunk = sender.clone();
    let sid_chunk = sid.clone();
    let chunk_flag = chunk_sent.clone();
    let streamed_content_chunk = streamed_content.clone();
    let on_chunk = move |chunk: String| {
        if abort_registry::is_aborted(&sid_chunk) {
            return;
        }
        chunk_flag.store(true, Ordering::Relaxed);
        if let Ok(mut acc) = streamed_content_chunk.lock() {
            *acc = join_stream_text(acc.as_str(), chunk.as_str());
        }
        sender_chunk.send_json(
            &json!({ "type": Events::CHUNK, "timestamp": crate::core::time::now_rfc3339(), "content": chunk }),
        );
    };

    let sender_thinking = sender.clone();
    let sid_thinking = sid.clone();
    let on_thinking = move |chunk: String| {
        if abort_registry::is_aborted(&sid_thinking) {
            return;
        }
        sender_thinking.send_json(
            &json!({ "type": Events::THINKING, "timestamp": crate::core::time::now_rfc3339(), "content": chunk }),
        );
    };

    let sender_tools_start = sender.clone();
    let sid_tools_start = sid.clone();
    let on_tools_start = move |tool_calls: Value| {
        if abort_registry::is_aborted(&sid_tools_start) {
            return;
        }
        sender_tools_start.send_json(&json!({ "type": Events::TOOLS_START, "timestamp": crate::core::time::now_rfc3339(), "data": { "tool_calls": tool_calls } }));
    };

    let sender_tools_stream = sender.clone();
    let sid_tools_stream = sid.clone();
    let on_tools_stream = move |result: Value| {
        if abort_registry::is_aborted(&sid_tools_stream) {
            return;
        }
        sender_tools_stream.send_json(
            &json!({ "type": Events::TOOLS_STREAM, "timestamp": crate::core::time::now_rfc3339(), "data": result }),
        );
    };

    let sender_tools_end = sender.clone();
    let sid_tools_end = sid.clone();
    let on_tools_end = move |result: Value| {
        if abort_registry::is_aborted(&sid_tools_end) {
            return;
        }
        sender_tools_end.send_json(
            &json!({ "type": Events::TOOLS_END, "timestamp": crate::core::time::now_rfc3339(), "data": result }),
        );
    };

    let sender_sum_start = sender.clone();
    let sid_sum_start = sid.clone();
    let on_sum_start = move |info: Value| {
        if abort_registry::is_aborted(&sid_sum_start) {
            return;
        }
        sender_sum_start.send_json(&json!({ "type": Events::CONTEXT_SUMMARIZED_START, "timestamp": crate::core::time::now_rfc3339(), "data": info }));
    };

    let sender_sum_stream = sender.clone();
    let sid_sum_stream = sid.clone();
    let on_sum_stream = move |chunk: Value| {
        if abort_registry::is_aborted(&sid_sum_stream) {
            return;
        }
        sender_sum_stream.send_json(&json!({ "type": Events::CONTEXT_SUMMARIZED_STREAM, "timestamp": crate::core::time::now_rfc3339(), "data": chunk }));
    };

    let sender_sum_end = sender.clone();
    let sid_sum_end = sid.clone();
    let on_sum_end = move |info: Value| {
        if abort_registry::is_aborted(&sid_sum_end) {
            return;
        }
        sender_sum_end.send_json(
            &json!({ "type": Events::CONTEXT_SUMMARIZED_END, "timestamp": crate::core::time::now_rfc3339(), "data": info }),
        );
    };

    let callbacks = V2AiClientCallbacks {
        on_chunk: Some(Arc::new(on_chunk)),
        on_thinking: Some(Arc::new(on_thinking)),
        on_tools_start: Some(Arc::new(on_tools_start)),
        on_tools_stream: Some(Arc::new(on_tools_stream)),
        on_tools_end: Some(Arc::new(on_tools_end)),
        on_context_summarized_start: Some(Arc::new(on_sum_start)),
        on_context_summarized_stream: Some(Arc::new(on_sum_stream)),
        on_context_summarized_end: Some(Arc::new(on_sum_end)),
    };

    StreamCallbacksV2 {
        callbacks,
        chunk_sent,
        streamed_content,
    }
}

pub fn build_v3_callbacks(
    sender: &SseSender,
    session_id: &str,
    enable_tools: bool,
) -> StreamCallbacksV3 {
    let sid = session_id.to_string();
    let chunk_sent = Arc::new(AtomicBool::new(false));
    let streamed_content = Arc::new(Mutex::new(String::new()));

    let sender_chunk = sender.clone();
    let sid_chunk = sid.clone();
    let chunk_flag = chunk_sent.clone();
    let streamed_content_chunk = streamed_content.clone();
    let on_chunk = move |chunk: String| {
        if abort_registry::is_aborted(&sid_chunk) {
            return;
        }
        chunk_flag.store(true, Ordering::Relaxed);
        if let Ok(mut acc) = streamed_content_chunk.lock() {
            *acc = join_stream_text(acc.as_str(), chunk.as_str());
        }
        sender_chunk.send_json(
            &json!({ "type": Events::CHUNK, "timestamp": crate::core::time::now_rfc3339(), "content": chunk }),
        );
    };

    let sender_thinking = sender.clone();
    let sid_thinking = sid.clone();
    let on_thinking = move |chunk: String| {
        if abort_registry::is_aborted(&sid_thinking) {
            return;
        }
        sender_thinking.send_json(
            &json!({ "type": Events::THINKING, "timestamp": crate::core::time::now_rfc3339(), "content": chunk }),
        );
    };

    let on_tools_start = if enable_tools {
        let sender_tools_start = sender.clone();
        let sid_tools_start = sid.clone();
        Some(Arc::new(move |tool_calls: Value| {
            if abort_registry::is_aborted(&sid_tools_start) {
                return;
            }
            sender_tools_start.send_json(&json!({ "type": Events::TOOLS_START, "timestamp": crate::core::time::now_rfc3339(), "data": { "tool_calls": tool_calls } }));
        }) as Arc<dyn Fn(Value) + Send + Sync>)
    } else {
        None
    };

    let on_tools_stream = if enable_tools {
        let sender_tools_stream = sender.clone();
        let sid_tools_stream = sid.clone();
        Some(Arc::new(move |result: Value| {
            if abort_registry::is_aborted(&sid_tools_stream) {
                return;
            }
            sender_tools_stream.send_json(
                &json!({ "type": Events::TOOLS_STREAM, "timestamp": crate::core::time::now_rfc3339(), "data": result }),
            );
        }) as Arc<dyn Fn(Value) + Send + Sync>)
    } else {
        None
    };

    let on_tools_end = if enable_tools {
        let sender_tools_end = sender.clone();
        let sid_tools_end = sid.clone();
        Some(Arc::new(move |result: Value| {
            if abort_registry::is_aborted(&sid_tools_end) {
                return;
            }
            sender_tools_end.send_json(
                &json!({ "type": Events::TOOLS_END, "timestamp": crate::core::time::now_rfc3339(), "data": result }),
            );
        }) as Arc<dyn Fn(Value) + Send + Sync>)
    } else {
        None
    };

    let sender_sum_start = sender.clone();
    let sid_sum_start = sid.clone();
    let on_sum_start = Arc::new(move |info: Value| {
        if abort_registry::is_aborted(&sid_sum_start) {
            return;
        }
        sender_sum_start.send_json(&json!({ "type": Events::CONTEXT_SUMMARIZED_START, "timestamp": crate::core::time::now_rfc3339(), "data": info }));
    }) as Arc<dyn Fn(Value) + Send + Sync>;

    let sender_sum_stream = sender.clone();
    let sid_sum_stream = sid.clone();
    let on_sum_stream = Arc::new(move |chunk: Value| {
        if abort_registry::is_aborted(&sid_sum_stream) {
            return;
        }
        sender_sum_stream.send_json(&json!({ "type": Events::CONTEXT_SUMMARIZED_STREAM, "timestamp": crate::core::time::now_rfc3339(), "data": chunk }));
    }) as Arc<dyn Fn(Value) + Send + Sync>;

    let sender_sum_end = sender.clone();
    let sid_sum_end = sid;
    let on_sum_end = Arc::new(move |info: Value| {
        if abort_registry::is_aborted(&sid_sum_end) {
            return;
        }
        sender_sum_end.send_json(&json!({ "type": Events::CONTEXT_SUMMARIZED_END, "timestamp": crate::core::time::now_rfc3339(), "data": info }));
    }) as Arc<dyn Fn(Value) + Send + Sync>;

    let callbacks = V3AiClientCallbacks {
        on_chunk: Some(Arc::new(on_chunk)),
        on_thinking: Some(Arc::new(on_thinking)),
        on_tools_start,
        on_tools_stream,
        on_tools_end,
        on_context_summarized_start: Some(on_sum_start),
        on_context_summarized_stream: Some(on_sum_stream),
        on_context_summarized_end: Some(on_sum_end),
    };

    StreamCallbacksV3 {
        callbacks,
        chunk_sent,
        streamed_content,
    }
}

pub fn send_fallback_chunk_if_needed(
    sender: &SseSender,
    chunk_sent: &Arc<AtomicBool>,
    result: &Value,
) {
    if chunk_sent.load(Ordering::Relaxed) {
        return;
    }

    if let Some(text) = result.get("content").and_then(|v| v.as_str()) {
        if !text.is_empty() {
            sender.send_json(
                &json!({ "type": Events::CHUNK, "timestamp": crate::core::time::now_rfc3339(), "content": text }),
            );
        }
    }
}

pub fn send_start_event(sender: &SseSender, session_id: &str) {
    sender.send_json(
        &json!({ "type": Events::START, "timestamp": crate::core::time::now_rfc3339(), "session_id": session_id }),
    );
}

pub fn send_complete_event(sender: &SseSender, result: &Value) {
    sender.send_json(
        &json!({ "type": Events::COMPLETE, "timestamp": crate::core::time::now_rfc3339(), "result": result }),
    );
}

pub fn send_cancelled_event(sender: &SseSender) {
    sender.send_json(
        &json!({ "type": Events::CANCELLED, "timestamp": crate::core::time::now_rfc3339() }),
    );
}

pub fn send_error_event(sender: &SseSender, error: &str) {
    sender.send_json(&json!({
        "type": Events::ERROR,
        "timestamp": crate::core::time::now_rfc3339(),
        "message": error,
        "data": {
            "error": error,
            "message": error
        }
    }));
}

pub fn handle_chat_result(
    sender: &SseSender,
    session_id: &str,
    chunk_sent: Option<&Arc<AtomicBool>>,
    streamed_content: Option<&Arc<Mutex<String>>>,
    result: Result<Value, String>,
    mut on_cancelled: impl FnMut(),
    mut on_error: impl FnMut(&str),
) -> bool {
    match result {
        Ok(res) => {
            if abort_registry::is_aborted(session_id) {
                on_cancelled();
                send_cancelled_event(sender);
                return false;
            }

            if let Some(flag) = chunk_sent {
                send_fallback_chunk_if_needed(sender, flag, &res);
            }
            let complete_result = ensure_complete_event_content(&res, streamed_content);
            send_complete_event(sender, &complete_result);
            true
        }
        Err(err) => {
            if abort_registry::is_aborted(session_id) {
                on_cancelled();
                send_cancelled_event(sender);
            } else {
                on_error(err.as_str());
                send_error_event(sender, &err);
            }
            false
        }
    }
}

fn ensure_complete_event_content(
    result: &Value,
    streamed_content: Option<&Arc<Mutex<String>>>,
) -> Value {
    let Some(streamed_content) = streamed_content else {
        return result.clone();
    };
    let streamed_text = streamed_content
        .lock()
        .ok()
        .map(|guard| guard.clone())
        .unwrap_or_default();
    let streamed_text = normalize_streamed_text(streamed_text.as_str());
    if streamed_text.is_empty() {
        return result.clone();
    }

    let result_content = result
        .get("content")
        .and_then(|value| value.as_str())
        .unwrap_or("");
    let normalized_result_content = normalize_streamed_text(result_content);
    let merged_content = if normalized_result_content.is_empty() {
        streamed_text
    } else {
        join_stream_text(streamed_text.as_str(), normalized_result_content.as_str())
    };

    let mut patched = result.clone();
    if let Some(obj) = patched.as_object_mut() {
        obj.insert("content".to_string(), Value::String(merged_content));
    }
    patched
}

fn normalize_streamed_text(value: &str) -> String {
    if value.is_empty() {
        return String::new();
    }
    value
        .replace("\r\n", "\n")
        .replace('\r', "\n")
        .replace("\n\n\n\n\n\n", "\n\n\n\n")
}

fn join_stream_text(current: &str, chunk: &str) -> String {
    if chunk.is_empty() {
        return current.to_string();
    }
    if current.is_empty() {
        return chunk.to_string();
    }

    if chunk.starts_with(current) {
        return chunk.to_string();
    }
    if current.starts_with(chunk) {
        return current.to_string();
    }

    let max_overlap = std::cmp::min(current.len(), chunk.len());
    for overlap in (8..=max_overlap).rev() {
        let Some(current_tail) = current.get(current.len() - overlap..) else {
            continue;
        };
        let Some(chunk_head) = chunk.get(..overlap) else {
            continue;
        };
        if current_tail == chunk_head {
            let rest = chunk.get(overlap..).unwrap_or_default();
            return format!("{}{}", current, rest);
        }
    }

    format!("{}{}", current, chunk)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ensure_complete_event_content_prefers_longer_streamed_text() {
        let acc = Arc::new(Mutex::new("你好，世界。完整内容".to_string()));
        let result = json!({
            "success": true,
            "content": "世界。完整内容"
        });

        let patched = ensure_complete_event_content(&result, Some(&acc));
        assert_eq!(
            patched.get("content").and_then(|v| v.as_str()),
            Some("你好，世界。完整内容")
        );
    }

    #[test]
    fn ensure_complete_event_content_keeps_longer_result_text() {
        let acc = Arc::new(Mutex::new("hello".to_string()));
        let result = json!({
            "success": true,
            "content": "hello world"
        });

        let patched = ensure_complete_event_content(&result, Some(&acc));
        assert_eq!(
            patched.get("content").and_then(|v| v.as_str()),
            Some("hello world")
        );
    }
}
