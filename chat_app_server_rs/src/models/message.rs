use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::FromRow;
use uuid::Uuid;

use crate::repositories::messages as repo;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub id: String,
    pub session_id: String,
    pub role: String,
    pub content: String,
    pub summary: Option<String>,
    pub tool_calls: Option<Value>,
    pub tool_call_id: Option<String>,
    pub reasoning: Option<String>,
    pub metadata: Option<Value>,
    pub created_at: String,
}

#[derive(Debug, FromRow)]
pub struct MessageRow {
    pub id: String,
    pub session_id: String,
    pub role: String,
    pub content: String,
    pub summary: Option<String>,
    pub tool_calls: Option<String>,
    pub tool_call_id: Option<String>,
    pub reasoning: Option<String>,
    pub metadata: Option<String>,
    pub created_at: String,
}

impl MessageRow {
    pub fn to_message(self) -> Message {
        Message {
            id: self.id,
            session_id: self.session_id,
            role: self.role,
            content: self.content,
            summary: self.summary,
            tool_calls: self
                .tool_calls
                .and_then(|v| serde_json::from_str::<Value>(&v).ok()),
            tool_call_id: self.tool_call_id,
            reasoning: self.reasoning,
            metadata: self
                .metadata
                .and_then(|v| serde_json::from_str::<Value>(&v).ok()),
            created_at: self.created_at,
        }
    }
}

impl Message {
    pub fn new(session_id: String, role: String, content: String) -> Message {
        Message {
            id: Uuid::new_v4().to_string(),
            session_id,
            role,
            content,
            summary: None,
            tool_calls: None,
            tool_call_id: None,
            reasoning: None,
            metadata: None,
            created_at: chrono::Utc::now().to_rfc3339(),
        }
    }
}

pub struct MessageService;

impl MessageService {
    pub async fn create(message: Message) -> Result<Message, String> {
        repo::create_message(&message).await
    }

    pub fn create_sync(message: Message) -> Result<Message, String> {
        repo::create_message_sync(&message)
    }

    pub async fn get_by_id(message_id: &str) -> Result<Option<Message>, String> {
        repo::get_message_by_id(message_id).await
    }

    pub async fn get_by_session(
        session_id: &str,
        limit: Option<i64>,
        offset: i64,
    ) -> Result<Vec<Message>, String> {
        repo::get_messages_by_session(session_id, limit, offset).await
    }

    pub async fn get_recent_by_session(
        session_id: &str,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<Message>, String> {
        repo::get_recent_messages_by_session(session_id, limit, offset).await
    }

    pub async fn get_by_session_after(
        session_id: &str,
        after_created_at: &str,
        limit: Option<i64>,
    ) -> Result<Vec<Message>, String> {
        repo::get_messages_by_session_after(session_id, after_created_at, limit).await
    }

    pub async fn delete(message_id: &str) -> Result<(), String> {
        repo::delete_message(message_id).await
    }

    pub async fn delete_by_session(session_id: &str) -> Result<(), String> {
        repo::delete_messages_by_session(session_id).await
    }

    pub async fn count_by_session(session_id: &str) -> Result<i64, String> {
        repo::count_messages_by_session(session_id).await
    }
}
