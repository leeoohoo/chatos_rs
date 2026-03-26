use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use serde_json::{json, Value};

use crate::utils::abort_registry;
use crate::utils::events::Events;
use crate::utils::sse::SseSender;

use super::text::ensure_complete_event_content;

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
