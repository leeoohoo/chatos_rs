use serde::Serialize;
use serde_json::Value;

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
        MessageOut {
            id: msg.id,
            conversation_id: msg.session_id.clone(),
            conversation_id_camel: msg.session_id,
            role: msg.role,
            content: msg.content,
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

pub fn optional_text_has_content(value: Option<&str>) -> bool {
    value.map(text_has_content).unwrap_or(false)
}

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

pub fn select_preferred_text<'a>(content: &'a str, reasoning: Option<&'a str>) -> Option<&'a str> {
    if text_has_content(content) {
        return Some(content);
    }

    reasoning.filter(|value| text_has_content(value))
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

pub fn flatten_text_value(value: &Value, object_keys: &[&str]) -> String {
    if let Some(text) = value.as_str() {
        return text.to_string();
    }

    if let Some(array) = value.as_array() {
        let mut out = Vec::new();
        for item in array {
            let text = flatten_text_value(item, object_keys);
            if !text.is_empty() {
                out.push(text);
            }
        }
        return out.join("");
    }

    let Some(object) = value.as_object() else {
        return String::new();
    };

    for key in object_keys {
        if let Some(inner) = object.get(*key) {
            let text = flatten_text_value(inner, object_keys);
            if !text.is_empty() {
                return text;
            }
        }
    }

    String::new()
}

pub fn extract_non_empty_text_value(value: &Value, object_keys: &[&str]) -> Option<String> {
    let text = flatten_text_value(value, object_keys);
    if text.is_empty() {
        None
    } else {
        Some(text)
    }
}

pub fn text_value_or_json(value: &Value, object_keys: &[&str]) -> String {
    if value.is_null() {
        return String::new();
    }

    extract_non_empty_text_value(value, object_keys).unwrap_or_else(|| value.to_string())
}

pub fn join_text_lines_or_json(value: &Value, object_keys: &[&str]) -> String {
    if let Some(text) = value.as_str() {
        return text.to_string();
    }

    if let Some(array) = value.as_array() {
        let mut lines = Vec::new();
        for item in array {
            let text = text_value_or_json(item, object_keys);
            if !text.is_empty() {
                lines.push(text);
            }
        }
        return lines.join("\n");
    }

    text_value_or_json(value, object_keys)
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
        extract_non_empty_text_value, flatten_text_value, is_session_summary_message,
        join_text_lines_or_json, message_metadata_string_alias, message_turn_id,
        object_string_alias, optional_text_has_content, owned_non_empty_text,
        select_preferred_text, text_has_content, text_value_or_json,
    };
    use crate::models::message::Message;

    #[test]
    fn flattens_text_values_using_configured_keys() {
        let value = json!([
            {"text": {"value": "hello"}},
            {"content": [{"delta": " world"}]}
        ]);
        assert_eq!(
            flatten_text_value(&value, &["text", "value", "content", "delta"]),
            "hello world"
        );
    }

    #[test]
    fn extracts_non_empty_text_or_none() {
        let value = json!({"output_text": {"value": "done"}});
        assert_eq!(
            extract_non_empty_text_value(
                &value,
                &["text", "value", "content", "output_text", "delta"]
            )
            .as_deref(),
            Some("done")
        );
        assert_eq!(
            extract_non_empty_text_value(&json!({"other": 1}), &["text", "value"]),
            None
        );
    }

    #[test]
    fn stringifies_text_or_json_with_configured_keys() {
        assert_eq!(
            text_value_or_json(&json!({"text": {"value": "done"}}), &["text", "value"]),
            "done"
        );
        assert_eq!(
            text_value_or_json(&json!({"other": 1}), &["text", "value"]),
            "{\"other\":1}"
        );
        assert_eq!(text_value_or_json(&Value::Null, &["text"]), "");
    }

    #[test]
    fn joins_text_lines_or_json_from_arrays() {
        let value = json!([
            {"text": "alpha"},
            {"other": 1},
            "omega"
        ]);
        assert_eq!(
            join_text_lines_or_json(&value, &["text"]),
            "alpha\n{\"other\":1}\nomega"
        );
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
