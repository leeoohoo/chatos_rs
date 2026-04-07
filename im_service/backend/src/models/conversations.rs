use serde::{Deserialize, Serialize};

use super::{default_active, default_i64_0};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImConversation {
    pub id: String,
    pub owner_user_id: String,
    pub contact_id: String,
    pub project_id: Option<String>,
    pub title: Option<String>,
    #[serde(default = "default_active")]
    pub status: String,
    pub last_message_at: Option<String>,
    pub last_message_preview: Option<String>,
    #[serde(default = "default_i64_0")]
    pub unread_count: i64,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateConversationRequest {
    pub owner_user_id: String,
    pub contact_id: String,
    pub project_id: Option<String>,
    pub title: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateConversationRequest {
    pub title: Option<String>,
    pub status: Option<String>,
    pub last_message_at: Option<String>,
    pub last_message_preview: Option<String>,
    pub unread_count: Option<i64>,
}
