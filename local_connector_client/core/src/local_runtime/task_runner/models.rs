// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde::Serialize;
use sqlx::FromRow;

#[derive(Debug, Clone)]
pub(crate) struct EnqueueLocalTaskRunInput {
    pub(crate) owner_user_id: String,
    pub(crate) project_id: String,
    pub(crate) requirement_id: Option<String>,
    pub(crate) task_kind: String,
    pub(crate) task_id: String,
    pub(crate) session_id: String,
    pub(crate) execution_group_id: String,
    pub(crate) priority: i64,
    pub(crate) prompt: String,
    pub(crate) model_config_id: String,
}

#[derive(Debug, Clone)]
pub(crate) struct CreateLocalConversationTaskInput {
    pub(crate) owner_user_id: String,
    pub(crate) project_id: String,
    pub(crate) session_id: String,
    pub(crate) source_turn_id: String,
    pub(crate) title: String,
    pub(crate) description: String,
    pub(crate) objective: String,
    pub(crate) priority: i64,
    pub(crate) tags: Vec<String>,
    pub(crate) model_config_id: String,
    pub(crate) is_planning_task: bool,
    pub(crate) enabled_builtin_kinds: Vec<String>,
    pub(crate) external_mcp_config_ids: Vec<String>,
    pub(crate) selected_skill_ids: Vec<String>,
    pub(crate) prerequisite_task_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, FromRow)]
pub(crate) struct LocalTaskRunRecord {
    pub(crate) id: String,
    pub(crate) owner_user_id: String,
    pub(crate) project_id: String,
    pub(crate) requirement_id: Option<String>,
    pub(crate) task_kind: String,
    pub(crate) task_id: String,
    pub(crate) session_id: String,
    pub(crate) turn_id: String,
    pub(crate) execution_group_id: String,
    pub(crate) status: String,
    pub(crate) priority: i64,
    pub(crate) prompt: String,
    pub(crate) model_config_id: String,
    pub(crate) attempt: i64,
    pub(crate) max_attempts: i64,
    pub(crate) worker_id: Option<String>,
    pub(crate) lease_expires_at: Option<String>,
    pub(crate) heartbeat_at: Option<String>,
    pub(crate) cancel_requested: bool,
    pub(crate) result_content: Option<String>,
    pub(crate) result_reasoning: Option<String>,
    pub(crate) tool_calls_json: Option<String>,
    pub(crate) finish_reason: Option<String>,
    pub(crate) usage_json: Option<String>,
    pub(crate) error: Option<String>,
    pub(crate) created_at: String,
    pub(crate) started_at: Option<String>,
    pub(crate) finished_at: Option<String>,
    pub(crate) updated_at: String,
}
