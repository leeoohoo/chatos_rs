use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use serde_json::{json, Value};

use crate::services::v2::ai_client::AiClientCallbacks as V2AiClientCallbacks;
use crate::services::v3::ai_client::AiClientCallbacks as V3AiClientCallbacks;
use crate::utils::abort_registry;
use crate::utils::events::Events;
use crate::utils::sse::SseSender;

pub struct StreamCallbacksV2 {
    pub callbacks: V2AiClientCallbacks,
    pub chunk_sent: Arc<AtomicBool>,
}

pub struct StreamCallbacksV3 {
    pub callbacks: V3AiClientCallbacks,
    pub chunk_sent: Arc<AtomicBool>,
}

pub fn build_v2_callbacks(sender: &SseSender, session_id: &str) -> StreamCallbacksV2 {
    let sid = session_id.to_string();
    let chunk_sent = Arc::new(AtomicBool::new(false));

    let sender_chunk = sender.clone();
    let sid_chunk = sid.clone();
    let chunk_flag = chunk_sent.clone();
    let on_chunk = move |chunk: String| {
        if abort_registry::is_aborted(&sid_chunk) {
            return;
        }
        chunk_flag.store(true, Ordering::Relaxed);
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
    }
}

pub fn build_v3_callbacks(
    sender: &SseSender,
    session_id: &str,
    enable_tools: bool,
) -> StreamCallbacksV3 {
    let sid = session_id.to_string();
    let chunk_sent = Arc::new(AtomicBool::new(false));

    let sender_chunk = sender.clone();
    let sid_chunk = sid.clone();
    let chunk_flag = chunk_sent.clone();
    let on_chunk = move |chunk: String| {
        if abort_registry::is_aborted(&sid_chunk) {
            return;
        }
        chunk_flag.store(true, Ordering::Relaxed);
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
    sender.send_json(
        &json!({ "type": Events::ERROR, "timestamp": crate::core::time::now_rfc3339(), "data": { "error": error } }),
    );
}

pub fn handle_chat_result(
    sender: &SseSender,
    session_id: &str,
    chunk_sent: Option<&Arc<AtomicBool>>,
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
            send_complete_event(sender, &res);
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
