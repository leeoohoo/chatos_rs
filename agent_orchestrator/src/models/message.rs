use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub id: String,
    pub session_id: String,
    pub role: String,
    pub content: String,
    pub message_mode: Option<String>,
    pub message_source: Option<String>,
    pub summary: Option<String>,
    pub tool_calls: Option<Value>,
    pub tool_call_id: Option<String>,
    pub reasoning: Option<String>,
    pub metadata: Option<Value>,
    pub created_at: String,
}

impl Message {
    pub fn new(session_id: String, role: String, content: String) -> Message {
        Message {
            id: Uuid::new_v4().to_string(),
            session_id,
            role,
            content,
            message_mode: None,
            message_source: None,
            summary: None,
            tool_calls: None,
            tool_call_id: None,
            reasoning: None,
            metadata: None,
            created_at: crate::core::time::now_rfc3339(),
        }
    }
}
