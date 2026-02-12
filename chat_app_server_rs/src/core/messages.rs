use serde::Serialize;
use serde_json::Value;

use crate::models::message::{Message, MessageService};
use crate::services::session_title::maybe_rename_session_title;

#[derive(Debug, Clone, Default)]
pub struct NewMessageFields {
    pub role: Option<String>,
    pub content: Option<String>,
    pub tool_calls: Option<Value>,
    pub tool_call_id: Option<String>,
    pub reasoning: Option<String>,
    pub metadata: Option<Value>,
}

#[derive(Debug, Serialize)]
pub struct MessageOut {
    pub id: String,
    #[serde(rename = "sessionId")]
    pub session_id: String,
    pub role: String,
    pub content: String,
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
            session_id: msg.session_id,
            role: msg.role,
            content: msg.content,
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
    message.tool_calls = fields.tool_calls;
    message.tool_call_id = fields.tool_call_id;
    message.reasoning = fields.reasoning;
    message.metadata = fields.metadata;
    message
}

pub async fn create_message_and_maybe_rename(message: Message) -> Result<Message, String> {
    let session_id = message.session_id.clone();
    let role = message.role.clone();
    let content = message.content.clone();

    let saved = MessageService::create(message).await?;
    if role == "user" {
        let _ = maybe_rename_session_title(&session_id, &content, 30).await;
    }
    Ok(saved)
}
