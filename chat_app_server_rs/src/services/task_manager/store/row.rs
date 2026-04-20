use sqlx::FromRow;

use crate::services::task_manager::normalizer::parse_tags_json;
use crate::services::task_manager::types::TaskRecord;

#[derive(Debug, Clone, FromRow)]
pub(super) struct TaskRow {
    pub(super) id: String,
    pub(super) conversation_id: String,
    pub(super) conversation_turn_id: String,
    pub(super) title: String,
    pub(super) details: String,
    pub(super) priority: String,
    pub(super) status: String,
    pub(super) tags_json: String,
    pub(super) due_at: Option<String>,
    pub(super) created_at: String,
    pub(super) updated_at: String,
}

impl TaskRow {
    pub(super) fn into_record(self) -> TaskRecord {
        TaskRecord {
            id: self.id,
            conversation_id: self.conversation_id,
            conversation_turn_id: self.conversation_turn_id,
            title: self.title,
            details: self.details,
            priority: self.priority,
            status: self.status,
            tags: parse_tags_json(self.tags_json.as_str()),
            due_at: self.due_at,
            created_at: self.created_at,
            updated_at: self.updated_at,
        }
    }
}
