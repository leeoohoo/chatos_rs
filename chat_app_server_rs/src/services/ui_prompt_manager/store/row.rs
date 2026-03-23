use sqlx::FromRow;

use crate::services::ui_prompt_manager::types::UiPromptRecord;

use super::codec::{parse_json_or_default, parse_status};

#[derive(Debug, Clone, FromRow)]
pub(super) struct UiPromptRow {
    pub(super) id: String,
    pub(super) session_id: String,
    pub(super) conversation_turn_id: String,
    pub(super) tool_call_id: Option<String>,
    pub(super) kind: String,
    pub(super) status: String,
    pub(super) prompt_json: String,
    pub(super) response_json: Option<String>,
    pub(super) expires_at: Option<String>,
    pub(super) created_at: String,
    pub(super) updated_at: String,
}

impl UiPromptRow {
    pub(super) fn into_record(self) -> UiPromptRecord {
        UiPromptRecord {
            id: self.id,
            session_id: self.session_id,
            conversation_turn_id: self.conversation_turn_id,
            tool_call_id: self.tool_call_id,
            kind: self.kind,
            status: parse_status(self.status.as_str()),
            prompt: parse_json_or_default(self.prompt_json.as_str()),
            response: self.response_json.as_deref().map(parse_json_or_default),
            expires_at: self.expires_at,
            created_at: self.created_at,
            updated_at: self.updated_at,
        }
    }
}
