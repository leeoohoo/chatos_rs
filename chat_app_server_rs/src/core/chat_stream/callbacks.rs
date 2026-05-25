use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use serde_json::{json, Value};

use crate::core::chat_stream::ChatEventSink;
use crate::services::v2::ai_client::AiClientCallbacks as V2AiClientCallbacks;
use crate::services::v3::ai_client::AiClientCallbacks as V3AiClientCallbacks;
use crate::utils::abort_registry;
use crate::utils::events::Events;

use super::text::join_stream_text;
use super::{StreamCallbacksV2, StreamCallbacksV3};

pub fn build_v2_callbacks(sink: &ChatEventSink, session_id: &str) -> StreamCallbacksV2 {
    let sid = session_id.to_string();
    let chunk_sent = Arc::new(AtomicBool::new(false));
    let streamed_content = Arc::new(Mutex::new(String::new()));

    let sink_chunk = sink.clone();
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
        sink_chunk.send_json_event(
            "chat.turn.delta",
            Events::CHUNK,
            json!({ "type": Events::CHUNK, "timestamp": crate::core::time::now_rfc3339(), "content": chunk }),
        );
    };

    let sink_thinking = sink.clone();
    let sid_thinking = sid.clone();
    let on_thinking = move |chunk: String| {
        if abort_registry::is_aborted(&sid_thinking) {
            return;
        }
        sink_thinking.send_json_event(
            "chat.turn.thinking",
            Events::THINKING,
            json!({ "type": Events::THINKING, "timestamp": crate::core::time::now_rfc3339(), "content": chunk }),
        );
    };

    let sink_turn_phase = sink.clone();
    let sid_turn_phase = sid.clone();
    let on_turn_phase = move |payload: Value| {
        if abort_registry::is_aborted(&sid_turn_phase) {
            return;
        }
        sink_turn_phase.send_json_event(
            "chat.turn.phase",
            Events::TURN_PHASE,
            json!({
                "type": Events::TURN_PHASE,
                "timestamp": crate::core::time::now_rfc3339(),
                "data": payload
            }),
        );
    };

    let sink_tools_start = sink.clone();
    let sid_tools_start = sid.clone();
    let on_tools_start = move |tool_calls: Value| {
        if abort_registry::is_aborted(&sid_tools_start) {
            return;
        }
        sink_tools_start.send_json_event(
            "chat.tool.started",
            Events::TOOLS_START,
            json!({ "type": Events::TOOLS_START, "timestamp": crate::core::time::now_rfc3339(), "data": { "tool_calls": tool_calls } }),
        );
    };

    let sink_tools_stream = sink.clone();
    let sid_tools_stream = sid.clone();
    let on_tools_stream = move |result: Value| {
        if abort_registry::is_aborted(&sid_tools_stream) {
            return;
        }
        sink_tools_stream.send_json_event(
            "chat.tool.delta",
            Events::TOOLS_STREAM,
            json!({ "type": Events::TOOLS_STREAM, "timestamp": crate::core::time::now_rfc3339(), "data": result }),
        );
    };

    let sink_tools_end = sink.clone();
    let sid_tools_end = sid.clone();
    let on_tools_end = move |result: Value| {
        if abort_registry::is_aborted(&sid_tools_end) {
            return;
        }
        sink_tools_end.send_json_event(
            "chat.tool.completed",
            Events::TOOLS_END,
            json!({ "type": Events::TOOLS_END, "timestamp": crate::core::time::now_rfc3339(), "data": result }),
        );
    };

    let sink_sum_start = sink.clone();
    let sid_sum_start = sid.clone();
    let on_sum_start = move |info: Value| {
        if abort_registry::is_aborted(&sid_sum_start) {
            return;
        }
        sink_sum_start.send_json_event(
            "chat.context_summarized.started",
            Events::CONTEXT_SUMMARIZED_START,
            json!({ "type": Events::CONTEXT_SUMMARIZED_START, "timestamp": crate::core::time::now_rfc3339(), "data": info }),
        );
    };

    let sink_sum_stream = sink.clone();
    let sid_sum_stream = sid.clone();
    let on_sum_stream = move |chunk: Value| {
        if abort_registry::is_aborted(&sid_sum_stream) {
            return;
        }
        sink_sum_stream.send_json_event(
            "chat.context_summarized.delta",
            Events::CONTEXT_SUMMARIZED_STREAM,
            json!({ "type": Events::CONTEXT_SUMMARIZED_STREAM, "timestamp": crate::core::time::now_rfc3339(), "data": chunk }),
        );
    };

    let sink_sum_end = sink.clone();
    let sid_sum_end = sid.clone();
    let on_sum_end = move |info: Value| {
        if abort_registry::is_aborted(&sid_sum_end) {
            return;
        }
        sink_sum_end.send_json_event(
            "chat.context_summarized.completed",
            Events::CONTEXT_SUMMARIZED_END,
            json!({ "type": Events::CONTEXT_SUMMARIZED_END, "timestamp": crate::core::time::now_rfc3339(), "data": info }),
        );
    };

    let sink_runtime_guidance = sink.clone();
    let sid_runtime_guidance = sid.clone();
    let on_runtime_guidance_applied = move |payload: Value| {
        if abort_registry::is_aborted(&sid_runtime_guidance) {
            return;
        }
        sink_runtime_guidance.send_json_event(
            "chat.runtime_guidance.applied",
            Events::RUNTIME_GUIDANCE_APPLIED,
            json!({
                "type": Events::RUNTIME_GUIDANCE_APPLIED,
                "timestamp": crate::core::time::now_rfc3339(),
                "data": payload
            }),
        );
    };

    let callbacks = V2AiClientCallbacks {
        on_chunk: Some(Arc::new(on_chunk)),
        on_thinking: Some(Arc::new(on_thinking)),
        on_turn_phase: Some(Arc::new(on_turn_phase)),
        on_tools_start: Some(Arc::new(on_tools_start)),
        on_tools_stream: Some(Arc::new(on_tools_stream)),
        on_tools_end: Some(Arc::new(on_tools_end)),
        on_runtime_guidance_applied: Some(Arc::new(on_runtime_guidance_applied)),
        on_context_summarized_start: Some(Arc::new(on_sum_start)),
        on_context_summarized_stream: Some(Arc::new(on_sum_stream)),
        on_context_summarized_end: Some(Arc::new(on_sum_end)),
        on_before_send_model_request: None,
        on_before_model_request: None,
    };

    StreamCallbacksV2 {
        callbacks,
        chunk_sent,
        streamed_content,
    }
}

pub fn build_v3_callbacks(
    sink: &ChatEventSink,
    session_id: &str,
    enable_tools: bool,
) -> StreamCallbacksV3 {
    let sid = session_id.to_string();
    let chunk_sent = Arc::new(AtomicBool::new(false));
    let streamed_content = Arc::new(Mutex::new(String::new()));

    let sink_chunk = sink.clone();
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
        sink_chunk.send_json_event(
            "chat.turn.delta",
            Events::CHUNK,
            json!({ "type": Events::CHUNK, "timestamp": crate::core::time::now_rfc3339(), "content": chunk }),
        );
    };

    let sink_thinking = sink.clone();
    let sid_thinking = sid.clone();
    let on_thinking = move |chunk: String| {
        if abort_registry::is_aborted(&sid_thinking) {
            return;
        }
        sink_thinking.send_json_event(
            "chat.turn.thinking",
            Events::THINKING,
            json!({ "type": Events::THINKING, "timestamp": crate::core::time::now_rfc3339(), "content": chunk }),
        );
    };

    let sink_turn_phase = sink.clone();
    let sid_turn_phase = session_id.to_string();
    let on_turn_phase = Arc::new(move |payload: Value| {
        if abort_registry::is_aborted(&sid_turn_phase) {
            return;
        }
        sink_turn_phase.send_json_event(
            "chat.turn.phase",
            Events::TURN_PHASE,
            json!({
                "type": Events::TURN_PHASE,
                "timestamp": crate::core::time::now_rfc3339(),
                "data": payload
            }),
        );
    }) as Arc<dyn Fn(Value) + Send + Sync>;

    let on_tools_start = if enable_tools {
        let sink_tools_start = sink.clone();
        let sid_tools_start = sid.clone();
        Some(Arc::new(move |tool_calls: Value| {
            if abort_registry::is_aborted(&sid_tools_start) {
                return;
            }
            sink_tools_start.send_json_event(
                "chat.tool.started",
                Events::TOOLS_START,
                json!({ "type": Events::TOOLS_START, "timestamp": crate::core::time::now_rfc3339(), "data": { "tool_calls": tool_calls } }),
            );
        }) as Arc<dyn Fn(Value) + Send + Sync>)
    } else {
        None
    };

    let on_tools_stream = if enable_tools {
        let sink_tools_stream = sink.clone();
        let sid_tools_stream = sid.clone();
        Some(Arc::new(move |result: Value| {
            if abort_registry::is_aborted(&sid_tools_stream) {
                return;
            }
            sink_tools_stream.send_json_event(
                "chat.tool.delta",
                Events::TOOLS_STREAM,
                json!({ "type": Events::TOOLS_STREAM, "timestamp": crate::core::time::now_rfc3339(), "data": result }),
            );
        }) as Arc<dyn Fn(Value) + Send + Sync>)
    } else {
        None
    };

    let on_tools_end = if enable_tools {
        let sink_tools_end = sink.clone();
        let sid_tools_end = sid.clone();
        Some(Arc::new(move |result: Value| {
            if abort_registry::is_aborted(&sid_tools_end) {
                return;
            }
            sink_tools_end.send_json_event(
                "chat.tool.completed",
                Events::TOOLS_END,
                json!({ "type": Events::TOOLS_END, "timestamp": crate::core::time::now_rfc3339(), "data": result }),
            );
        }) as Arc<dyn Fn(Value) + Send + Sync>)
    } else {
        None
    };

    let sink_sum_start = sink.clone();
    let sid_sum_start = sid.clone();
    let on_sum_start = Arc::new(move |info: Value| {
        if abort_registry::is_aborted(&sid_sum_start) {
            return;
        }
        sink_sum_start.send_json_event(
            "chat.context_summarized.started",
            Events::CONTEXT_SUMMARIZED_START,
            json!({ "type": Events::CONTEXT_SUMMARIZED_START, "timestamp": crate::core::time::now_rfc3339(), "data": info }),
        );
    }) as Arc<dyn Fn(Value) + Send + Sync>;

    let sink_sum_stream = sink.clone();
    let sid_sum_stream = sid.clone();
    let on_sum_stream = Arc::new(move |chunk: Value| {
        if abort_registry::is_aborted(&sid_sum_stream) {
            return;
        }
        sink_sum_stream.send_json_event(
            "chat.context_summarized.delta",
            Events::CONTEXT_SUMMARIZED_STREAM,
            json!({ "type": Events::CONTEXT_SUMMARIZED_STREAM, "timestamp": crate::core::time::now_rfc3339(), "data": chunk }),
        );
    }) as Arc<dyn Fn(Value) + Send + Sync>;

    let sink_sum_end = sink.clone();
    let sid_sum_end = sid;
    let on_sum_end = Arc::new(move |info: Value| {
        if abort_registry::is_aborted(&sid_sum_end) {
            return;
        }
        sink_sum_end.send_json_event(
            "chat.context_summarized.completed",
            Events::CONTEXT_SUMMARIZED_END,
            json!({ "type": Events::CONTEXT_SUMMARIZED_END, "timestamp": crate::core::time::now_rfc3339(), "data": info }),
        );
    }) as Arc<dyn Fn(Value) + Send + Sync>;

    let sink_runtime_guidance = sink.clone();
    let sid_runtime_guidance = session_id.to_string();
    let on_runtime_guidance_applied = Arc::new(move |payload: Value| {
        if abort_registry::is_aborted(&sid_runtime_guidance) {
            return;
        }
        sink_runtime_guidance.send_json_event(
            "chat.runtime_guidance.applied",
            Events::RUNTIME_GUIDANCE_APPLIED,
            json!({
                "type": Events::RUNTIME_GUIDANCE_APPLIED,
                "timestamp": crate::core::time::now_rfc3339(),
                "data": payload
            }),
        );
    }) as Arc<dyn Fn(Value) + Send + Sync>;

    let callbacks = V3AiClientCallbacks {
        on_chunk: Some(Arc::new(on_chunk)),
        on_thinking: Some(Arc::new(on_thinking)),
        on_turn_phase: Some(on_turn_phase),
        on_tools_start,
        on_tools_stream,
        on_tools_end,
        on_runtime_guidance_applied: Some(on_runtime_guidance_applied),
        on_context_summarized_start: Some(on_sum_start),
        on_context_summarized_stream: Some(on_sum_stream),
        on_context_summarized_end: Some(on_sum_end),
        on_before_send_model_request: None,
        on_before_model_request: None,
    };

    StreamCallbacksV3 {
        callbacks,
        chunk_sent,
        streamed_content,
    }
}
