// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde::Serialize;
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::core::tool_call::extract_message_tool_calls;
use crate::models::message::Message;
use crate::services::chatos_sessions;
use crate::services::session_title::maybe_rename_session_title;

#[derive(Debug, Clone, Default)]
pub struct NewMessageFields {
    pub role: Option<String>,
    pub content: Option<String>,
    pub message_mode: Option<String>,
    pub message_source: Option<String>,
    pub tool_calls: Option<Value>,
    pub tool_call_id: Option<String>,
    pub reasoning: Option<String>,
    pub metadata: Option<Value>,
}

#[derive(Debug, Serialize)]
pub struct MessageOut {
    pub id: String,
    pub revision: i64,
    pub sequence_no: i64,
    pub conversation_id: String,
    #[serde(rename = "conversationId")]
    pub conversation_id_camel: String,
    pub role: String,
    pub content: String,
    pub message_mode: Option<String>,
    pub message_source: Option<String>,
    pub summary: Option<String>,
    #[serde(rename = "toolCalls")]
    pub tool_calls: Option<Value>,
    pub tool_call_id: Option<String>,
    pub reasoning: Option<String>,
    pub metadata: Option<Value>,
    pub summary_status: String,
    pub summary_id: Option<String>,
    pub summarized_at: Option<String>,
    pub created_at: String,
}

impl From<Message> for MessageOut {
    fn from(msg: Message) -> Self {
        let content = msg.content.clone();
        let revision = msg.revision();
        let sequence_no = message_sequence_no(&msg);
        MessageOut {
            id: msg.id,
            revision,
            sequence_no,
            conversation_id: msg.session_id.clone(),
            conversation_id_camel: msg.session_id,
            role: msg.role,
            content,
            message_mode: msg.message_mode,
            message_source: msg.message_source,
            summary: msg.summary,
            tool_calls: msg.tool_calls,
            tool_call_id: msg.tool_call_id,
            reasoning: msg.reasoning,
            metadata: msg.metadata,
            summary_status: msg.summary_status,
            summary_id: msg.summary_id,
            summarized_at: msg.summarized_at,
            created_at: msg.created_at,
        }
    }
}

fn message_sequence_no(message: &Message) -> i64 {
    let timestamp_micros = chrono::DateTime::parse_from_rfc3339(message.created_at.as_str())
        .map(|value| value.timestamp_micros())
        .unwrap_or_default()
        .max(0);
    let digest = Sha256::digest(message.id.as_bytes());
    let tie_breaker = i64::from(u16::from_be_bytes([digest[0], digest[1]]) & 0x03ff);
    timestamp_micros
        .saturating_mul(1_024)
        .saturating_add(tie_breaker)
}

pub fn build_message(session_id: String, fields: NewMessageFields, default_role: &str) -> Message {
    let role = fields.role.unwrap_or_else(|| default_role.to_string());
    let content = fields.content.unwrap_or_default();

    let mut message = Message::new(session_id, role, content);
    message.message_mode = fields.message_mode;
    message.message_source = fields.message_source;
    message.tool_calls = fields.tool_calls;
    message.tool_call_id = fields.tool_call_id;
    message.reasoning = fields.reasoning;
    message.metadata = fields.metadata;
    message
}

pub fn ensure_message_metadata_object(
    message: &mut Message,
) -> &mut serde_json::Map<String, Value> {
    if !matches!(message.metadata, Some(Value::Object(_))) {
        message.metadata = Some(Value::Object(serde_json::Map::new()));
    }

    match message.metadata {
        Some(Value::Object(ref mut map)) => map,
        _ => unreachable!("metadata should be object"),
    }
}

pub fn text_has_content(value: &str) -> bool {
    !value.trim().is_empty()
}

#[cfg(test)]
pub fn optional_text_has_content(value: Option<&str>) -> bool {
    value.map(text_has_content).unwrap_or(false)
}

#[cfg(test)]
pub fn owned_non_empty_text(value: &str) -> Option<String> {
    text_has_content(value).then(|| value.to_string())
}

pub fn message_has_text_content(message: &Message) -> bool {
    text_has_content(&message.content)
}

pub fn is_session_summary_message(message: &Message) -> bool {
    message
        .metadata
        .as_ref()
        .and_then(|metadata| metadata.get("type"))
        .and_then(|value| value.as_str())
        == Some("session_summary")
}

pub fn is_runtime_guidance_message(message: &Message) -> bool {
    message
        .message_mode
        .as_deref()
        .map(str::trim)
        .is_some_and(|mode| mode == "runtime_guidance")
        || message
            .message_source
            .as_deref()
            .map(str::trim)
            .is_some_and(|source| source == "runtime_guidance")
        || message
            .metadata
            .as_ref()
            .and_then(|metadata| metadata.get("runtime_guidance"))
            .is_some()
}

pub fn is_runtime_guidance_user_message(message: &Message) -> bool {
    message.role == "user" && is_runtime_guidance_message(message)
}

pub fn message_is_hidden(message: &Message) -> bool {
    message
        .metadata
        .as_ref()
        .and_then(|metadata| metadata.get("hidden"))
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

pub fn extract_message_tool_calls_for_display(message: &Message) -> Vec<Value> {
    extract_message_tool_calls(message.tool_calls.as_ref(), message.metadata.as_ref())
}

#[cfg(test)]
pub fn select_preferred_text<'a>(content: &'a str, reasoning: Option<&'a str>) -> Option<&'a str> {
    chatos_model_transport::select_preferred_response_text(content, reasoning)
}

pub async fn create_message_and_maybe_rename(message: Message) -> Result<Message, String> {
    let session_id = message.session_id.clone();
    let role = message.role.clone();
    let content = message.content.clone();

    let saved = chatos_sessions::upsert_message(&message).await?;
    if role == "user" {
        let _ = maybe_rename_session_title(&session_id, &content, 30).await;
    }
    Ok(saved)
}

pub fn object_string_alias<'a>(value: &'a Value, keys: &[&str]) -> Option<&'a str> {
    let object = value.as_object()?;
    keys.iter()
        .find_map(|key| object.get(*key))
        .and_then(Value::as_str)
}

pub fn message_metadata_string_alias<'a>(message: &'a Message, keys: &[&str]) -> Option<&'a str> {
    message
        .metadata
        .as_ref()
        .and_then(|metadata| object_string_alias(metadata, keys))
}

pub fn message_turn_id(message: &Message) -> Option<&str> {
    message_metadata_string_alias(message, &["conversation_turn_id", "conversationTurnId"])
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

#[cfg(test)]
mod tests {
    use serde_json::{json, Value};

    use super::{
        ensure_message_metadata_object, extract_message_tool_calls_for_display,
        is_session_summary_message, message_metadata_string_alias, message_turn_id,
        object_string_alias, optional_text_has_content, owned_non_empty_text,
        select_preferred_text, text_has_content, MessageOut,
    };
    use crate::models::message::Message;

    #[test]
    fn message_output_exposes_revision_and_stable_sequence() {
        let mut message = Message::new(
            "session-1".to_string(),
            "user".to_string(),
            "hello".to_string(),
        );
        message.id = "message-1".to_string();
        message.created_at = "2026-08-24T03:00:00Z".to_string();
        message.set_revision(8);

        let first = MessageOut::from(message.clone());
        let second = MessageOut::from(message);
        assert_eq!(first.revision, 8);
        assert_eq!(first.sequence_no, second.sequence_no);
        assert!(first.sequence_no > 0);
    }

    #[test]
    fn detects_non_empty_text_content() {
        assert!(text_has_content(" hello "));
        assert!(!text_has_content("   "));
        assert!(optional_text_has_content(Some("world")));
        assert!(!optional_text_has_content(Some("\n\t")));
        assert!(!optional_text_has_content(None));
        assert_eq!(owned_non_empty_text(" hello "), Some(" hello ".to_string()));
        assert_eq!(owned_non_empty_text("   "), None);
    }

    #[test]
    fn identifies_session_summary_messages() {
        let mut summary = Message::new(
            "session_1".to_string(),
            "assistant".to_string(),
            "summary".to_string(),
        );
        summary.metadata = Some(json!({"type": "session_summary"}));

        let normal = Message::new(
            "session_1".to_string(),
            "assistant".to_string(),
            "reply".to_string(),
        );

        assert!(is_session_summary_message(&summary));
        assert!(!is_session_summary_message(&normal));
    }

    #[test]
    fn selects_content_then_reasoning_text() {
        assert_eq!(
            select_preferred_text("hello", Some("thinking")),
            Some("hello")
        );
        assert_eq!(
            select_preferred_text("   ", Some("thinking")),
            Some("thinking")
        );
        assert_eq!(select_preferred_text("   ", Some("   ")), None);
        assert_eq!(select_preferred_text("", None), None);
    }

    #[test]
    fn ensures_message_metadata_object_and_preserves_existing_map() {
        let mut message = Message::new(
            "session_1".to_string(),
            "assistant".to_string(),
            "".to_string(),
        );
        ensure_message_metadata_object(&mut message).insert(
            "conversation_turn_id".to_string(),
            Value::String("turn_1".to_string()),
        );

        let metadata = ensure_message_metadata_object(&mut message);
        assert_eq!(
            metadata.get("conversation_turn_id").and_then(Value::as_str),
            Some("turn_1")
        );
    }

    #[test]
    fn resolves_object_and_message_metadata_aliases() {
        let metadata = json!({
            "responseId": "resp_1",
            "conversationTurnId": "turn_1"
        });
        assert_eq!(
            object_string_alias(&metadata, &["response_id", "responseId"]),
            Some("resp_1")
        );

        let mut message = Message::new(
            "session_1".to_string(),
            "assistant".to_string(),
            "".to_string(),
        );
        message.metadata = Some(metadata);
        assert_eq!(
            message_metadata_string_alias(&message, &["response_id", "responseId"]),
            Some("resp_1")
        );
        assert_eq!(message_turn_id(&message), Some("turn_1"));
    }

    #[test]
    fn extracts_message_tool_calls_from_message_or_metadata() {
        let mut message = Message::new(
            "session_1".to_string(),
            "assistant".to_string(),
            "tool".to_string(),
        );
        message.metadata = Some(json!({
            "toolCalls": [{"id":"call_1"}]
        }));

        let calls = extract_message_tool_calls_for_display(&message);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].get("id").and_then(Value::as_str), Some("call_1"));
    }
}
