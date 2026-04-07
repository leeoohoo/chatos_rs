use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;
use crate::core::chat_runtime::selection_from_metadata;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub id: String,
    pub title: String,
    pub description: Option<String>,
    pub metadata: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub selected_model_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub selected_agent_id: Option<String>,
    pub user_id: Option<String>,
    pub project_id: Option<String>,
    pub status: String,
    pub archived_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

impl Session {
    pub fn new(
        title: String,
        description: Option<String>,
        metadata: Option<Value>,
        user_id: Option<String>,
        project_id: Option<String>,
    ) -> Session {
        let now = crate::core::time::now_rfc3339();
        let (selected_model_id, selected_agent_id) = selection_from_metadata(metadata.as_ref());
        Session {
            id: Uuid::new_v4().to_string(),
            title,
            description,
            metadata,
            selected_model_id,
            selected_agent_id,
            user_id,
            project_id,
            status: "active".to_string(),
            archived_at: None,
            created_at: now.clone(),
            updated_at: now,
        }
    }
}
