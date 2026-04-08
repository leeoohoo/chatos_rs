use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use serde_json::{json, Value};

use crate::utils::abort_registry;
use crate::utils::chat_event_sender::ChatEventSender;
use crate::utils::events::Events;

pub fn send_fallback_chunk_if_needed(
    sender: &impl ChatEventSender,
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

pub fn send_start_event(sender: &impl ChatEventSender, session_id: &str) {
    sender.send_json(
        &json!({ "type": Events::START, "timestamp": crate::core::time::now_rfc3339(), "session_id": session_id }),
    );
}

pub fn send_complete_event(sender: &impl ChatEventSender, result: &Value) {
    sender.send_json(
        &json!({ "type": Events::COMPLETE, "timestamp": crate::core::time::now_rfc3339(), "result": result }),
    );
}

pub fn send_cancelled_event(sender: &impl ChatEventSender) {
    sender.send_json(
        &json!({ "type": Events::CANCELLED, "timestamp": crate::core::time::now_rfc3339() }),
    );
}

pub fn send_error_event(sender: &impl ChatEventSender, error: &str) {
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
    sender: &impl ChatEventSender,
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
            let _ = streamed_content;
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
