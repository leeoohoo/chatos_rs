use serde::Deserialize;
use serde_json::Value;

#[derive(Debug, Deserialize, Clone)]
pub(crate) struct ChatStreamRequest {
    #[serde(rename = "conversation_id", alias = "conversationId")]
    pub conversation_id: Option<String>,
    pub content: Option<String>,
    pub model_config_id: Option<String>,
    pub ai_model_config: Option<Value>,
    pub user_id: Option<String>,
    pub attachments: Option<Vec<Value>>,
    pub reasoning_enabled: Option<bool>,
    pub turn_id: Option<String>,
    pub contact_agent_id: Option<String>,
    pub project_id: Option<String>,
    pub project_root: Option<String>,
    #[serde(alias = "workspaceRoot")]
    pub workspace_root: Option<String>,
    pub remote_connection_id: Option<String>,
    #[serde(skip_deserializing)]
    pub user_message_id: Option<String>,
}
