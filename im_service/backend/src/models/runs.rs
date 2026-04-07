use serde::{Deserialize, Serialize};

use super::default_pending;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationRun {
    pub id: String,
    pub conversation_id: String,
    pub source_message_id: String,
    pub contact_id: String,
    pub agent_id: String,
    pub project_id: Option<String>,
    pub execution_session_id: Option<String>,
    pub execution_turn_id: Option<String>,
    pub execution_scope_key: Option<String>,
    #[serde(default = "default_pending")]
    pub status: String,
    pub final_message_id: Option<String>,
    pub error_message: Option<String>,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateConversationRunRequest {
    pub conversation_id: String,
    pub source_message_id: String,
    pub contact_id: String,
    pub agent_id: String,
    pub project_id: Option<String>,
    pub execution_session_id: Option<String>,
    pub execution_turn_id: Option<String>,
    pub execution_scope_key: Option<String>,
    pub status: Option<String>,
    pub started_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateConversationRunRequest {
    pub status: Option<String>,
    pub final_message_id: Option<String>,
    pub error_message: Option<String>,
    pub execution_session_id: Option<String>,
    pub execution_turn_id: Option<String>,
    pub execution_scope_key: Option<String>,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
}
