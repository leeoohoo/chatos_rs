use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::default_sending;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationMessage {
    pub id: String,
    pub conversation_id: String,
    pub sender_type: String,
    pub sender_id: Option<String>,
    pub message_type: String,
    pub content: String,
    #[serde(default = "default_sending")]
    pub delivery_status: String,
    pub client_message_id: Option<String>,
    pub reply_to_message_id: Option<String>,
    pub metadata: Option<Value>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateConversationMessageRequest {
    pub sender_type: String,
    pub sender_id: Option<String>,
    pub message_type: Option<String>,
    pub content: String,
    pub delivery_status: Option<String>,
    pub client_message_id: Option<String>,
    pub reply_to_message_id: Option<String>,
    pub metadata: Option<Value>,
}
