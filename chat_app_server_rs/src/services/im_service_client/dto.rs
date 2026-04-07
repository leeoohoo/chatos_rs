use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Deserialize)]
pub struct ImAuthLoginResponse {
    pub token: String,
    pub username: String,
    pub display_name: String,
    pub role: String,
    pub status: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ImAuthMeResponse {
    pub id: String,
    pub username: String,
    pub display_name: String,
    pub avatar_url: Option<String>,
    pub role: String,
    pub status: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ImContactDto {
    pub id: String,
    pub owner_user_id: String,
    pub agent_id: String,
    pub display_name: String,
    pub avatar_url: Option<String>,
    pub status: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CreateImContactRequestDto {
    pub owner_user_id: Option<String>,
    pub agent_id: String,
    pub display_name: String,
    pub avatar_url: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ImConversationDto {
    pub id: String,
    pub owner_user_id: String,
    pub contact_id: String,
    pub project_id: Option<String>,
    pub title: Option<String>,
    pub status: String,
    pub last_message_at: Option<String>,
    pub last_message_preview: Option<String>,
    pub unread_count: i64,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CreateConversationRequestDto {
    pub owner_user_id: Option<String>,
    pub contact_id: String,
    pub project_id: Option<String>,
    pub title: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct UpdateConversationRequestDto {
    pub title: Option<String>,
    pub status: Option<String>,
    pub last_message_at: Option<String>,
    pub last_message_preview: Option<String>,
    pub unread_count: Option<i64>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ConversationMessageDto {
    pub id: String,
    pub conversation_id: String,
    pub sender_type: String,
    pub sender_id: Option<String>,
    pub message_type: String,
    pub content: String,
    pub delivery_status: String,
    pub client_message_id: Option<String>,
    pub reply_to_message_id: Option<String>,
    pub metadata: Option<Value>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CreateConversationMessageRequestDto {
    pub sender_type: String,
    pub sender_id: Option<String>,
    pub message_type: Option<String>,
    pub content: String,
    pub delivery_status: Option<String>,
    pub client_message_id: Option<String>,
    pub reply_to_message_id: Option<String>,
    pub metadata: Option<Value>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ConversationActionRequestDto {
    pub id: String,
    pub conversation_id: String,
    pub trigger_message_id: Option<String>,
    pub run_id: Option<String>,
    pub action_type: String,
    pub status: String,
    pub payload: Value,
    pub submitted_payload: Option<Value>,
    pub created_at: String,
    pub updated_at: String,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Serialize)]
pub struct CreateConversationActionRequestDto {
    pub conversation_id: String,
    pub trigger_message_id: Option<String>,
    pub run_id: Option<String>,
    pub action_type: String,
    pub status: Option<String>,
    pub payload: Value,
    pub submitted_payload: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct UpdateConversationActionRequestDto {
    pub status: Option<String>,
    pub submitted_payload: Option<Value>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ConversationRunDto {
    pub id: String,
    pub conversation_id: String,
    pub source_message_id: String,
    pub contact_id: String,
    pub agent_id: String,
    pub project_id: Option<String>,
    pub execution_session_id: Option<String>,
    pub execution_turn_id: Option<String>,
    pub execution_scope_key: Option<String>,
    pub status: String,
    pub final_message_id: Option<String>,
    pub error_message: Option<String>,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct CreateConversationRunRequestDto {
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

#[derive(Debug, Clone, Serialize, Default)]
pub struct UpdateConversationRunRequestDto {
    pub status: Option<String>,
    pub final_message_id: Option<String>,
    pub error_message: Option<String>,
    pub execution_session_id: Option<String>,
    pub execution_turn_id: Option<String>,
    pub execution_scope_key: Option<String>,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PublishConversationEventRequestDto {
    pub owner_user_id: String,
    pub event_type: String,
    pub conversation_id: String,
    pub field_name: String,
    pub payload: Value,
}
