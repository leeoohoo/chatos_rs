// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_mcp::TaskOutcomeItem;
use serde::Serialize;
use sqlx::FromRow;

#[derive(Debug, Clone, FromRow)]
pub(crate) struct LocalTaskBoardTaskRow {
    pub(crate) id: String,
    pub(crate) session_id: String,
    pub(crate) turn_id: String,
    pub(crate) source_user_message_id: Option<String>,
    pub(crate) title: String,
    pub(crate) details: String,
    pub(crate) priority: String,
    pub(crate) status: String,
    pub(crate) tags_json: String,
    pub(crate) prerequisite_task_ids_json: String,
    pub(crate) due_at: Option<String>,
    pub(crate) outcome_summary: String,
    pub(crate) outcome_items_json: String,
    pub(crate) resume_hint: String,
    pub(crate) blocker_reason: String,
    pub(crate) blocker_needs_json: String,
    pub(crate) blocker_kind: String,
    pub(crate) completed_at: Option<String>,
    pub(crate) last_outcome_at: Option<String>,
    pub(crate) created_at: String,
    pub(crate) updated_at: String,
    pub(crate) task_kind: String,
    pub(crate) objective: String,
    pub(crate) model_config_id: Option<String>,
    pub(crate) is_planning_task: bool,
    pub(crate) enabled_builtin_kinds_json: String,
    pub(crate) external_mcp_config_ids_json: String,
    pub(crate) selected_skill_ids_json: String,
    pub(crate) last_run_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct LocalTaskBoardTaskRecord {
    pub(crate) id: String,
    pub(crate) conversation_id: String,
    pub(crate) conversation_turn_id: String,
    pub(crate) source_session_id: String,
    pub(crate) source_turn_id: String,
    pub(crate) source_user_message_id: Option<String>,
    pub(crate) title: String,
    pub(crate) details: String,
    pub(crate) priority: String,
    pub(crate) status: String,
    pub(crate) tags: Vec<String>,
    pub(crate) prerequisite_task_ids: Vec<String>,
    pub(crate) due_at: Option<String>,
    pub(crate) outcome_summary: String,
    pub(crate) outcome_items: Vec<TaskOutcomeItem>,
    pub(crate) resume_hint: String,
    pub(crate) blocker_reason: String,
    pub(crate) blocker_needs: Vec<String>,
    pub(crate) blocker_kind: String,
    pub(crate) completed_at: Option<String>,
    pub(crate) last_outcome_at: Option<String>,
    pub(crate) created_at: String,
    pub(crate) updated_at: String,
    pub(crate) task_kind: String,
    pub(crate) objective: String,
    pub(crate) model_config_id: Option<String>,
    pub(crate) is_planning_task: bool,
    pub(crate) enabled_builtin_kinds: Vec<String>,
    pub(crate) external_mcp_config_ids: Vec<String>,
    pub(crate) selected_skill_ids: Vec<String>,
    pub(crate) last_run_id: Option<String>,
}

impl From<LocalTaskBoardTaskRow> for LocalTaskBoardTaskRecord {
    fn from(row: LocalTaskBoardTaskRow) -> Self {
        Self {
            id: row.id,
            conversation_id: row.session_id.clone(),
            conversation_turn_id: row.turn_id.clone(),
            source_session_id: row.session_id,
            source_turn_id: row.turn_id,
            source_user_message_id: row.source_user_message_id,
            title: row.title,
            details: row.details,
            priority: row.priority,
            status: row.status,
            tags: parse_json(row.tags_json.as_str()),
            prerequisite_task_ids: parse_json(row.prerequisite_task_ids_json.as_str()),
            due_at: row.due_at,
            outcome_summary: row.outcome_summary,
            outcome_items: parse_json(row.outcome_items_json.as_str()),
            resume_hint: row.resume_hint,
            blocker_reason: row.blocker_reason,
            blocker_needs: parse_json(row.blocker_needs_json.as_str()),
            blocker_kind: row.blocker_kind,
            completed_at: row.completed_at,
            last_outcome_at: row.last_outcome_at,
            created_at: row.created_at,
            updated_at: row.updated_at,
            task_kind: row.task_kind,
            objective: row.objective,
            model_config_id: row.model_config_id,
            is_planning_task: row.is_planning_task,
            enabled_builtin_kinds: parse_json(row.enabled_builtin_kinds_json.as_str()),
            external_mcp_config_ids: parse_json(row.external_mcp_config_ids_json.as_str()),
            selected_skill_ids: parse_json(row.selected_skill_ids_json.as_str()),
            last_run_id: row.last_run_id,
        }
    }
}

fn parse_json<T: serde::de::DeserializeOwned + Default>(raw: &str) -> T {
    serde_json::from_str(raw).unwrap_or_default()
}
