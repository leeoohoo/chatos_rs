use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use serde_json::{json, Value};

use crate::services::realtime::publish_chat_stream_event;
use crate::utils::abort_registry;
use crate::utils::events::Events;
use crate::utils::sse::SseSender;

#[derive(Clone, Debug, Default)]
pub struct ChatRealtimeStreamContext {
    pub user_id: Option<String>,
    pub conversation_id: Option<String>,
    pub conversation_turn_id: Option<String>,
    pub project_id: Option<String>,
    pub user_message_id: Option<String>,
}

#[derive(Clone)]
pub struct ChatEventSink {
    sse: Option<SseSender>,
    realtime: Option<ChatRealtimeStreamContext>,
}

impl ChatEventSink {
    pub fn new(sse: Option<SseSender>, realtime: Option<ChatRealtimeStreamContext>) -> Self {
        Self { sse, realtime }
    }

    pub fn send_json_event(
        &self,
        event_name: &'static str,
        stream_type: &str,
        payload: Value,
    ) {
        if let Some(sender) = &self.sse {
            sender.send_json(&payload);
        }

        if let Some(context) = &self.realtime {
            let Some(user_id) = context.user_id.as_deref() else {
                return;
            };
            let Some(conversation_id) = context.conversation_id.as_deref() else {
                return;
            };
            publish_chat_stream_event(
                user_id,
                conversation_id,
                context.conversation_turn_id.as_deref(),
                context.project_id.as_deref(),
                context.user_message_id.as_deref(),
                event_name,
                stream_type,
                payload,
            );
        }
    }

    pub fn send_done(&self) {
        if let Some(sender) = &self.sse {
            sender.send_done();
        }
    }
}

pub fn send_fallback_chunk_if_needed(
    sink: &ChatEventSink,
    chunk_sent: &Arc<AtomicBool>,
    result: &Value,
) {
    if chunk_sent.load(Ordering::Relaxed) {
        return;
    }

    if let Some(text) = result.get("content").and_then(|v| v.as_str()) {
        if !text.is_empty() {
            sink.send_json_event(
                "chat.turn.delta",
                Events::CHUNK,
                json!({ "type": Events::CHUNK, "timestamp": crate::core::time::now_rfc3339(), "content": text }),
            );
        }
    }
}

pub fn send_start_event(sink: &ChatEventSink, conversation_id: &str) {
    sink.send_json_event(
        "chat.turn.started",
        Events::START,
        json!({ "type": Events::START, "timestamp": crate::core::time::now_rfc3339(), "conversation_id": conversation_id }),
    );
}

pub fn send_tools_unavailable_event(sink: &ChatEventSink, unavailable_tools: &[Value]) {
    if unavailable_tools.is_empty() {
        return;
    }
    sink.send_json_event(
        "chat.tools.unavailable",
        Events::TOOLS_UNAVAILABLE,
        json!({
            "type": Events::TOOLS_UNAVAILABLE,
            "timestamp": crate::core::time::now_rfc3339(),
            "data": {
                "unavailable_tools": unavailable_tools
            }
        }),
    );
}

pub fn send_complete_event(sink: &ChatEventSink, result: &Value) {
    sink.send_json_event(
        "chat.turn.completed",
        Events::COMPLETE,
        json!({ "type": Events::COMPLETE, "timestamp": crate::core::time::now_rfc3339(), "result": result }),
    );
}

pub fn send_cancelled_event(sink: &ChatEventSink) {
    sink.send_json_event(
        "chat.turn.cancelled",
        Events::CANCELLED,
        json!({ "type": Events::CANCELLED, "timestamp": crate::core::time::now_rfc3339() }),
    );
}

pub fn send_error_event(sink: &ChatEventSink, error: &str) {
    sink.send_json_event(
        "chat.turn.failed",
        Events::ERROR,
        json!({
            "type": Events::ERROR,
            "timestamp": crate::core::time::now_rfc3339(),
            "message": error,
            "data": {
                "error": error,
                "message": error
            }
        }),
    );
}

pub fn handle_chat_result(
    sink: &ChatEventSink,
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
                send_cancelled_event(sink);
                return false;
            }

            if let Some(flag) = chunk_sent {
                send_fallback_chunk_if_needed(sink, flag, &res);
            }
            let _ = streamed_content;
            send_complete_event(sink, &res);
            true
        }
        Err(err) => {
            if abort_registry::is_aborted(session_id) {
                on_cancelled();
                send_cancelled_event(sink);
            } else {
                on_error(err.as_str());
                send_error_event(sink, &err);
            }
            false
        }
    }
}
