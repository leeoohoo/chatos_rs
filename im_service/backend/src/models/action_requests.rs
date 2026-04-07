use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::default_pending;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationActionRequest {
    pub id: String,
    pub conversation_id: String,
    pub trigger_message_id: Option<String>,
    pub run_id: Option<String>,
    pub action_type: String,
    #[serde(default = "default_pending")]
    pub status: String,
    pub payload: Value,
    pub submitted_payload: Option<Value>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateConversationActionRequest {
    pub conversation_id: String,
    pub trigger_message_id: Option<String>,
    pub run_id: Option<String>,
    pub action_type: String,
    pub status: Option<String>,
    pub payload: Value,
    pub submitted_payload: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateConversationActionRequest {
    pub status: Option<String>,
    pub submitted_payload: Option<Value>,
}
