use serde::Serialize;
use serde_json::{json, Value};

use crate::models::message::Message;
use crate::services::memory_server_client;
use crate::services::ai_common::{
    extract_response_id_from_metadata, extract_response_status_from_metadata,
    is_non_terminal_response_status,
};
use crate::services::session_title::maybe_rename_session_title;
use crate::core::tool_call::extract_message_tool_calls;

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

pub fn build_assistant_role_message(content: Value) -> Value {
    json!({
        "role": "assistant",
        "content": content
    })
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

pub fn build_assistant_message_with_parts(
    content: Value,
    reasoning: Option<&str>,
    preserve_empty_reasoning: bool,
    tool_calls: Option<Value>,
) -> Value {
    let mut message = build_assistant_role_message(content);
    attach_reasoning_content(&mut message, reasoning, preserve_empty_reasoning);
    attach_message_tool_calls(&mut message, tool_calls);
    message
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

pub fn message_has_reasoning_content(message: &Message) -> bool {
    optional_text_has_content(message.reasoning.as_deref())
}

pub fn is_session_summary_message(message: &Message) -> bool {
    message
        .metadata
        .as_ref()
        .and_then(|metadata| metadata.get("type"))
        .and_then(|value| value.as_str())
        == Some("session_summary")
}

pub fn assistant_message_has_reusable_payload(message: &Message) -> bool {
    message.role == "assistant"
        && (message_has_text_content(message) || message_has_reasoning_content(message))
}

pub fn assistant_message_response_id_candidate<'a>(message: &'a Message) -> Option<&'a str> {
    if message.role != "assistant" {
        return None;
    }

    let tool_calls =
        extract_message_tool_calls(message.tool_calls.as_ref(), message.metadata.as_ref());
    if !tool_calls.is_empty() {
        return None;
    }

    let response_status = message
        .metadata
        .as_ref()
        .and_then(extract_response_status_from_metadata);
    if is_non_terminal_response_status(response_status) {
        return None;
    }

    if !assistant_message_has_reusable_payload(message) {
        return None;
    }

    message
        .metadata
        .as_ref()
        .and_then(extract_response_id_from_metadata)
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

pub fn attach_message_tool_calls(message: &mut Value, tool_calls: Option<Value>) {
    if let Some(tool_calls) = tool_calls.filter(|value| !value.is_null()) {
        message["tool_calls"] = tool_calls;
    }
}

pub fn attach_reasoning_content(
    message: &mut Value,
    reasoning: Option<&str>,
    include_when_empty: bool,
) {
    if include_when_empty {
        message["reasoning_content"] = Value::String(reasoning.unwrap_or_default().to_string());
        return;
    }

    if let Some(value) = reasoning.filter(|value| !value.trim().is_empty()) {
        message["reasoning_content"] = Value::String(value.to_string());
    }
}

pub async fn create_message_and_maybe_rename(message: Message) -> Result<Message, String> {
    let session_id = message.session_id.clone();
    let role = message.role.clone();
    let content = message.content.clone();

    let saved = memory_server_client::upsert_message(&message).await?;
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
        assistant_message_has_reusable_payload, assistant_message_response_id_candidate,
        attach_message_tool_calls, attach_reasoning_content, build_assistant_message_with_parts,
        build_assistant_role_message, ensure_message_metadata_object,
        extract_message_tool_calls_for_display, extract_non_empty_text_value, flatten_text_value,
        is_session_summary_message, join_text_lines_or_json, message_has_reasoning_content,
        message_has_text_content, message_metadata_string_alias, message_turn_id,
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
    fn builds_assistant_message_and_attaches_optional_fields() {
        let mut message = build_assistant_role_message(Value::Null);
        attach_reasoning_content(&mut message, Some("think"), false);
        attach_message_tool_calls(
            &mut message,
            Some(json!([{
                "id": "call_1",
                "type": "function",
                "function": {"name": "demo", "arguments": "{}"}
            }])),
        );

        assert_eq!(
            message.get("role").and_then(|value| value.as_str()),
            Some("assistant")
        );
        assert_eq!(message.get("content"), Some(&Value::Null));
        assert_eq!(
            message
                .get("reasoning_content")
                .and_then(|value| value.as_str()),
            Some("think")
        );
        assert_eq!(
            message
                .get("tool_calls")
                .and_then(|value| value.as_array())
                .map(|items| items.len()),
            Some(1)
        );
    }

    #[test]
    fn attach_reasoning_content_can_preserve_empty_reasoning() {
        let mut message = build_assistant_role_message(Value::Null);
        attach_reasoning_content(&mut message, None, true);
        assert_eq!(
            message
                .get("reasoning_content")
                .and_then(|value| value.as_str()),
            Some("")
        );
    }

    #[test]
    fn builds_assistant_message_with_shared_optional_parts() {
        let message = build_assistant_message_with_parts(
            Value::String("hello".to_string()),
            Some("think"),
            true,
            Some(json!([{
                "id": "call_1",
                "type": "function",
                "function": {"name": "demo", "arguments": "{}"}
            }])),
        );

        assert_eq!(
            message.get("content").and_then(|value| value.as_str()),
            Some("hello")
        );
        assert_eq!(
            message
                .get("reasoning_content")
                .and_then(|value| value.as_str()),
            Some("think")
        );
        assert_eq!(
            message
                .get("tool_calls")
                .and_then(|value| value.as_array())
                .map(|items| items.len()),
            Some(1)
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
    fn detects_reusable_assistant_payload_without_changing_role_rules() {
        let mut assistant_reasoning = Message::new(
            "session_1".to_string(),
            "assistant".to_string(),
            "   ".to_string(),
        );
        assistant_reasoning.reasoning = Some(" think ".to_string());

        let assistant_empty = Message::new(
            "session_1".to_string(),
            "assistant".to_string(),
            "".to_string(),
        );

        let system_message = Message::new(
            "session_1".to_string(),
            "system".to_string(),
            "reply".to_string(),
        );

        assert!(message_has_reasoning_content(&assistant_reasoning));
        assert!(assistant_message_has_reusable_payload(&assistant_reasoning));
        assert!(!message_has_text_content(&assistant_empty));
        assert!(!assistant_message_has_reusable_payload(&assistant_empty));
        assert!(!assistant_message_has_reusable_payload(&system_message));
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
    fn response_id_candidate_accepts_terminal_assistant_with_reusable_payload() {
        let mut message = Message::new(
            "session_1".to_string(),
            "assistant".to_string(),
            "final answer".to_string(),
        );
        message.metadata = Some(json!({
            "response_id": "resp_ok",
            "response_status": "completed",
        }));

        assert_eq!(assistant_message_response_id_candidate(&message), Some("resp_ok"));
    }

    #[test]
    fn response_id_candidate_rejects_tool_calls_non_terminal_and_empty_payloads() {
        let mut tool_call_message = Message::new(
            "session_1".to_string(),
            "assistant".to_string(),
            "tool call".to_string(),
        );
        tool_call_message.metadata = Some(json!({
            "response_id": "resp_tool",
            "response_status": "completed",
            "toolCalls": [{"id":"call_1"}]
        }));

        let mut non_terminal = Message::new(
            "session_1".to_string(),
            "assistant".to_string(),
            "pending".to_string(),
        );
        non_terminal.metadata = Some(json!({
            "response_id": "resp_pending",
            "response_status": "in_progress",
        }));

        let mut empty_payload = Message::new(
            "session_1".to_string(),
            "assistant".to_string(),
            "".to_string(),
        );
        empty_payload.metadata = Some(json!({
            "response_id": "resp_empty",
            "response_status": "completed",
        }));

        assert_eq!(assistant_message_response_id_candidate(&tool_call_message), None);
        assert_eq!(assistant_message_response_id_candidate(&non_terminal), None);
        assert_eq!(assistant_message_response_id_candidate(&empty_payload), None);
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
