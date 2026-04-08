use crate::core::chat_runtime::selection_from_metadata;
use crate::models::session::Session;

use super::MemorySession;

pub fn map_memory_session(value: MemorySession) -> Session {
    let (selected_model_id, selected_agent_id) = selection_from_metadata(value.metadata.as_ref());
    Session {
        id: value.id,
        title: value.title.unwrap_or_else(|| "Untitled".to_string()),
        description: None,
        metadata: value.metadata,
        selected_model_id,
        selected_agent_id,
        user_id: Some(value.user_id),
        project_id: value.project_id,
        status: value.status,
        archived_at: value.archived_at,
        created_at: value.created_at,
        updated_at: value.updated_at,
    }
}
