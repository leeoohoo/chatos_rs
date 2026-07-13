// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use serde_json::{json, Value};

use crate::core::messages::message_turn_id;
use crate::models::message::Message;
use crate::services::ai_common::classify_user_facing_ai_error;
use crate::services::chatos_sessions;
use crate::services::realtime::publish_chat_stream_event;
use crate::utils::abort_registry;
use crate::utils::events::Events;
use crate::utils::sse::SseSender;

const PERSISTED_TURN_MESSAGES_PAGE_SIZE: i64 = 200;

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

    pub fn send_json_event(&self, event_name: &'static str, stream_type: &str, payload: Value) {
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

async fn resolve_persisted_turn_messages(
    conversation_id: &str,
    conversation_turn_id: Option<&str>,
    user_message_id: Option<&str>,
) -> Option<(Message, Option<Message>)> {
    let turn_id = conversation_turn_id
        .map(str::trim)
        .filter(|value| !value.is_empty())?;

    let normalized_user_message_id = user_message_id
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let mut user_message =
        resolve_persisted_user_message_by_id(conversation_id, normalized_user_message_id).await;
    let mut assistant_message = None;
    let mut offset = 0_i64;

    loop {
        let batch = chatos_sessions::list_messages_including_hidden(
            conversation_id,
            Some(PERSISTED_TURN_MESSAGES_PAGE_SIZE),
            offset,
            false,
        )
        .await
        .ok()?;
        let batch_len = batch.len();
        if batch_len == 0 {
            break;
        }

        select_persisted_turn_messages_from_desc_page(
            batch,
            turn_id,
            normalized_user_message_id,
            &mut user_message,
            &mut assistant_message,
        );
        if user_message.is_some() && assistant_message.is_some() {
            break;
        }

        offset += batch_len as i64;
        if batch_len < PERSISTED_TURN_MESSAGES_PAGE_SIZE as usize {
            break;
        }
    }

    user_message.map(|user_message| (user_message, assistant_message))
}

async fn resolve_persisted_user_message_by_id(
    conversation_id: &str,
    user_message_id: Option<&str>,
) -> Option<Message> {
    let user_message_id = user_message_id?;
    let session = chatos_sessions::get_session_by_id(conversation_id)
        .await
        .ok()
        .flatten()?;
    chatos_sessions::get_message_by_id_in_session(&session, user_message_id)
        .await
        .ok()
        .flatten()
        .filter(|message| message.role == "user" && !message_hidden(message))
}

pub(super) fn select_persisted_turn_messages_from_desc_page(
    messages: impl IntoIterator<Item = Message>,
    turn_id: &str,
    user_message_id: Option<&str>,
    user_message: &mut Option<Message>,
    assistant_message: &mut Option<Message>,
) {
    for message in messages {
        if message_hidden(&message) {
            continue;
        }

        let role = message.role.as_str();
        let message_matches_turn = message_turn_id(&message) == Some(turn_id);
        if user_message.is_none()
            && role == "user"
            && (user_message_id == Some(message.id.as_str()) || message_matches_turn)
        {
            *user_message = Some(message.clone());
        }

        if assistant_message.is_none() && role == "assistant" && message_matches_turn {
            *assistant_message = Some(message);
        }

        if user_message.is_some() && assistant_message.is_some() {
            break;
        }
    }
}

fn message_hidden(message: &Message) -> bool {
    message
        .metadata
        .as_ref()
        .and_then(|metadata| metadata.get("hidden"))
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

pub async fn enrich_chat_result_with_persisted_messages(
    conversation_id: &str,
    conversation_turn_id: Option<&str>,
    user_message_id: Option<&str>,
    result: Value,
) -> Value {
    let Some((user_message, assistant_message)) =
        resolve_persisted_turn_messages(conversation_id, conversation_turn_id, user_message_id)
            .await
    else {
        return result;
    };

    let mut map = match result {
        Value::Object(map) => map,
        other => {
            let mut map = serde_json::Map::new();
            map.insert("value".to_string(), other);
            map
        }
    };

    map.insert(
        "persisted_user_message".to_string(),
        serde_json::to_value(&user_message).unwrap_or(Value::Null),
    );
    map.insert(
        "persisted_user_message_id".to_string(),
        Value::String(user_message.id.clone()),
    );
    if let Some(assistant_message) = assistant_message {
        map.insert(
            "persisted_assistant_message".to_string(),
            serde_json::to_value(&assistant_message).unwrap_or(Value::Null),
        );
        map.insert(
            "persisted_assistant_message_id".to_string(),
            Value::String(assistant_message.id.clone()),
        );
    }

    Value::Object(map)
}

pub async fn build_chat_turn_persisted_messages_payload(
    conversation_id: &str,
    conversation_turn_id: Option<&str>,
    user_message_id: Option<&str>,
) -> Option<Value> {
    enrich_chat_result_with_persisted_messages(
        conversation_id,
        conversation_turn_id,
        user_message_id,
        Value::Object(serde_json::Map::new()),
    )
    .await
    .as_object()
    .cloned()
    .map(Value::Object)
}

pub fn send_cancelled_event(sink: &ChatEventSink, result: Option<&Value>) {
    let mut payload = serde_json::Map::new();
    payload.insert(
        "type".to_string(),
        Value::String(Events::CANCELLED.to_string()),
    );
    payload.insert(
        "timestamp".to_string(),
        Value::String(crate::core::time::now_rfc3339()),
    );
    if let Some(result) = result {
        payload.insert("result".to_string(), result.clone());
    }
    sink.send_json_event(
        "chat.turn.cancelled",
        Events::CANCELLED,
        Value::Object(payload),
    );
}

pub(super) fn build_error_event_payload(error: &str, result: Option<&Value>) -> Value {
    let (message, code, detail) = match classify_user_facing_ai_error(error) {
        Some((code, message)) => (message, Some(code.to_string()), Some(error.to_string())),
        None => (error.to_string(), None, None),
    };
    let mut payload = serde_json::Map::new();
    payload.insert("type".to_string(), Value::String(Events::ERROR.to_string()));
    payload.insert(
        "timestamp".to_string(),
        Value::String(crate::core::time::now_rfc3339()),
    );
    payload.insert("message".to_string(), Value::String(message.clone()));
    if let Some(code) = code.clone() {
        payload.insert("code".to_string(), Value::String(code));
    }
    payload.insert(
        "data".to_string(),
        json!({
            "error": message,
            "message": message,
            "code": code,
            "detail": detail
        }),
    );
    if let Some(result) = result {
        payload.insert("result".to_string(), result.clone());
    }
    Value::Object(payload)
}

pub fn send_error_event(sink: &ChatEventSink, error: &str, result: Option<&Value>) {
    sink.send_json_event(
        "chat.turn.failed",
        Events::ERROR,
        build_error_event_payload(error, result),
    );
}

pub async fn handle_chat_result(
    sink: &ChatEventSink,
    session_id: &str,
    conversation_turn_id: Option<&str>,
    user_message_id: Option<&str>,
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
                let persisted_result = build_chat_turn_persisted_messages_payload(
                    session_id,
                    conversation_turn_id,
                    user_message_id,
                )
                .await;
                send_cancelled_event(sink, persisted_result.as_ref());
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
                let persisted_result = build_chat_turn_persisted_messages_payload(
                    session_id,
                    conversation_turn_id,
                    user_message_id,
                )
                .await;
                send_cancelled_event(sink, persisted_result.as_ref());
            } else {
                on_error(err.as_str());
                let persisted_result = build_chat_turn_persisted_messages_payload(
                    session_id,
                    conversation_turn_id,
                    user_message_id,
                )
                .await;
                send_error_event(sink, &err, persisted_result.as_ref());
            }
            false
        }
    }
}
