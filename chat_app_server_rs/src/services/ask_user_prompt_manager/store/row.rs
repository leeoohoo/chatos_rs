use sqlx::FromRow;

use crate::services::ask_user_prompt_manager::types::AskUserPromptRecord;

use super::codec::{parse_json_or_default, parse_status};

#[derive(Debug, Clone, FromRow)]
pub(super) struct AskUserPromptRow {
    pub(super) id: String,
    pub(super) conversation_id: String,
    pub(super) conversation_turn_id: String,
    pub(super) tool_call_id: Option<String>,
    pub(super) kind: String,
    pub(super) status: String,
    pub(super) prompt_json: String,
    pub(super) response_json: Option<String>,
    pub(super) expires_at: Option<String>,
    pub(super) source: Option<String>,
    pub(super) external_prompt_id: Option<String>,
    pub(super) external_task_id: Option<String>,
    pub(super) external_run_id: Option<String>,
    pub(super) external_project_id: Option<String>,
    pub(super) created_at: String,
    pub(super) updated_at: String,
}

impl AskUserPromptRow {
    pub(super) fn into_record(self) -> AskUserPromptRecord {
        AskUserPromptRecord {
            id: self.id,
            conversation_id: self.conversation_id,
            conversation_turn_id: self.conversation_turn_id,
            tool_call_id: self.tool_call_id,
            kind: self.kind,
            status: parse_status(self.status.as_str()),
            prompt: parse_json_or_default(self.prompt_json.as_str()),
            response: self.response_json.as_deref().map(parse_json_or_default),
            expires_at: self.expires_at,
            source: self
                .source
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .unwrap_or("chatos")
                .to_string(),
            external_prompt_id: self.external_prompt_id,
            external_task_id: self.external_task_id,
            external_run_id: self.external_run_id,
            external_project_id: self.external_project_id,
            created_at: self.created_at,
            updated_at: self.updated_at,
        }
    }
}
