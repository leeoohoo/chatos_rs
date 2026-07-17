// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde::Serialize;
use serde_json::Value;
use sqlx::FromRow;

#[derive(Debug, Clone, Serialize)]
pub(crate) struct LocalRuntimeDatabaseHealth {
    pub(crate) ready: bool,
    pub(crate) path: String,
    pub(crate) sqlite_version: String,
    pub(crate) applied_migrations: i64,
}

#[derive(Debug, Clone)]
pub(crate) struct UpsertLocalProjectInput {
    pub(crate) project_id: String,
    pub(crate) owner_user_id: String,
    pub(crate) device_id: String,
    pub(crate) workspace_id: String,
    pub(crate) project_name: String,
    pub(crate) root_relative_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, FromRow)]
pub(crate) struct LocalProjectRecord {
    pub(crate) project_id: String,
    pub(crate) owner_user_id: String,
    pub(crate) device_id: String,
    pub(crate) workspace_id: String,
    pub(crate) project_name: String,
    pub(crate) root_relative_path: Option<String>,
    pub(crate) execution_plane: String,
    pub(crate) runtime_schema_version: i64,
    pub(crate) created_at: String,
    pub(crate) updated_at: String,
}

#[derive(Debug, Clone)]
pub(crate) struct CreateLocalSessionInput {
    pub(crate) project_id: String,
    pub(crate) owner_user_id: String,
    pub(crate) title: String,
    pub(crate) selected_model_id: Option<String>,
    pub(crate) selected_agent_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, FromRow)]
pub(crate) struct LocalSessionRecord {
    pub(crate) id: String,
    pub(crate) project_id: String,
    pub(crate) owner_user_id: String,
    pub(crate) title: String,
    pub(crate) selected_model_id: Option<String>,
    pub(crate) selected_agent_id: Option<String>,
    pub(crate) status: String,
    pub(crate) message_count: i64,
    pub(crate) created_at: String,
    pub(crate) updated_at: String,
}

#[derive(Debug, Clone, Serialize, FromRow)]
pub(crate) struct LocalMessageRecord {
    pub(crate) id: String,
    pub(crate) session_id: String,
    pub(crate) turn_id: Option<String>,
    pub(crate) sequence_no: i64,
    pub(crate) role: String,
    pub(crate) content: String,
    pub(crate) reasoning: Option<String>,
    pub(crate) tool_calls_json: Option<String>,
    pub(crate) tool_call_id: Option<String>,
    pub(crate) metadata_json: Option<String>,
    pub(crate) created_at: String,
}

#[derive(Debug, Clone)]
pub(crate) struct AppendLocalMessageInput {
    pub(crate) session_id: String,
    pub(crate) owner_user_id: String,
    pub(crate) turn_id: String,
    pub(crate) message_id: Option<String>,
    pub(crate) role: String,
    pub(crate) content: String,
    pub(crate) reasoning: Option<String>,
    pub(crate) tool_calls_json: Option<String>,
    pub(crate) tool_call_id: Option<String>,
    pub(crate) metadata_json: Option<String>,
    pub(crate) created_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, FromRow)]
pub(crate) struct LocalTurnRecord {
    pub(crate) id: String,
    pub(crate) session_id: String,
    pub(crate) user_message_id: Option<String>,
    pub(crate) idempotency_key: String,
    pub(crate) status: String,
    pub(crate) cancel_requested: bool,
    pub(crate) error_code: Option<String>,
    pub(crate) error_message: Option<String>,
    pub(crate) started_at: Option<String>,
    pub(crate) finished_at: Option<String>,
    pub(crate) created_at: String,
    pub(crate) updated_at: String,
}

#[derive(Debug, Clone)]
pub(crate) struct BeginLocalTurnInput {
    pub(crate) session_id: String,
    pub(crate) owner_user_id: String,
    pub(crate) turn_id: String,
    pub(crate) idempotency_key: String,
    pub(crate) content: String,
    pub(crate) metadata_json: Option<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct CompleteLocalTurnInput {
    pub(crate) turn_id: String,
    pub(crate) owner_user_id: String,
    pub(crate) content: String,
    pub(crate) reasoning: Option<String>,
    pub(crate) tool_calls_json: Option<String>,
    pub(crate) metadata_json: Option<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct LocalTurnSnapshot {
    pub(crate) turn: LocalTurnRecord,
    pub(crate) user_message: LocalMessageRecord,
    pub(crate) assistant_message: Option<LocalMessageRecord>,
}

#[derive(Debug, Clone)]
pub(crate) enum BeginLocalTurnResult {
    Started(LocalTurnSnapshot),
    Existing(LocalTurnSnapshot),
}

#[derive(Debug, Clone, Serialize, FromRow)]
pub(crate) struct LocalRuntimeSettingsRecord {
    pub(crate) session_id: String,
    pub(crate) selected_model_id: Option<String>,
    pub(crate) selected_model_name: Option<String>,
    pub(crate) selected_thinking_level: Option<String>,
    pub(crate) workspace_root: Option<String>,
    pub(crate) reasoning_enabled: bool,
    pub(crate) plan_mode_enabled: bool,
    pub(crate) mcp_enabled: bool,
    pub(crate) enabled_mcp_ids_json: String,
    pub(crate) selected_skill_ids_json: String,
    pub(crate) auto_create_task: bool,
    pub(crate) memory_auto_summary_enabled: bool,
    pub(crate) memory_summary_message_threshold: i64,
    pub(crate) memory_summary_character_threshold: i64,
    pub(crate) memory_recall_limit: i64,
    pub(crate) created_at: String,
    pub(crate) updated_at: String,
}

#[derive(Debug, Clone)]
pub(crate) struct SaveLocalRuntimeSettingsInput {
    pub(crate) session_id: String,
    pub(crate) selected_model_id: Option<String>,
    pub(crate) selected_model_name: Option<String>,
    pub(crate) selected_thinking_level: Option<String>,
    pub(crate) workspace_root: Option<String>,
    pub(crate) reasoning_enabled: bool,
    pub(crate) plan_mode_enabled: bool,
    pub(crate) mcp_enabled: bool,
    pub(crate) enabled_mcp_ids_json: String,
    pub(crate) selected_skill_ids_json: String,
    pub(crate) auto_create_task: bool,
    pub(crate) memory_auto_summary_enabled: bool,
    pub(crate) memory_summary_message_threshold: i64,
    pub(crate) memory_summary_character_threshold: i64,
    pub(crate) memory_recall_limit: i64,
}

#[derive(Debug, Clone)]
pub(crate) struct AppendLocalRuntimeEventInput {
    pub(crate) owner_user_id: String,
    pub(crate) session_id: String,
    pub(crate) turn_id: String,
    pub(crate) event_name: String,
    pub(crate) stream_type: Option<String>,
    pub(crate) payload: Value,
}

#[derive(Debug, Clone, Serialize, FromRow)]
pub(crate) struct LocalRuntimeEventRecord {
    pub(crate) event_seq: i64,
    pub(crate) event_id: String,
    pub(crate) project_id: Option<String>,
    pub(crate) session_id: Option<String>,
    pub(crate) turn_id: Option<String>,
    pub(crate) event_name: String,
    pub(crate) stream_type: Option<String>,
    pub(crate) payload_json: String,
    pub(crate) created_at: String,
}

#[derive(Debug, Clone, Serialize, FromRow)]
pub(crate) struct LocalMemorySummaryRecord {
    pub(crate) id: String,
    pub(crate) session_id: String,
    pub(crate) summary_text: String,
    pub(crate) summary_model: String,
    pub(crate) trigger_type: String,
    pub(crate) source_start_message_id: Option<String>,
    pub(crate) source_end_message_id: Option<String>,
    pub(crate) source_message_count: i64,
    pub(crate) source_estimated_tokens: i64,
    pub(crate) level: i64,
    pub(crate) status: String,
    pub(crate) error_message: Option<String>,
    pub(crate) created_at: String,
    pub(crate) updated_at: String,
}

#[derive(Debug, Clone)]
pub(crate) struct CreateLocalMemorySummaryInput {
    pub(crate) owner_user_id: String,
    pub(crate) session_id: String,
    pub(crate) summary_text: String,
    pub(crate) summary_model: String,
    pub(crate) trigger_type: String,
    pub(crate) source_start_message_id: Option<String>,
    pub(crate) source_end_message_id: Option<String>,
    pub(crate) source_message_count: i64,
    pub(crate) source_estimated_tokens: i64,
    pub(crate) level: i64,
}

#[derive(Debug, Clone)]
pub(crate) struct LocalMemoryContext {
    pub(crate) summary: Option<LocalMemorySummaryRecord>,
    pub(crate) recalls: Vec<LocalSubjectMemoryRecord>,
    pub(crate) messages: Vec<LocalMessageRecord>,
}

#[derive(Debug, Clone, Serialize, FromRow)]
pub(crate) struct LocalSubjectMemoryRecord {
    pub(crate) id: String,
    pub(crate) subject_type: String,
    pub(crate) subject_id: String,
    pub(crate) project_id: String,
    pub(crate) recall_key: String,
    pub(crate) recall_text: String,
    pub(crate) source_session_id: String,
    pub(crate) source_summary_id: String,
    pub(crate) level: i64,
    pub(crate) confidence: Option<f64>,
    pub(crate) last_seen_at: Option<String>,
    pub(crate) created_at: String,
    pub(crate) updated_at: String,
}

#[derive(Debug, Clone)]
pub(crate) struct LocalSubjectMemoryRollupPlan {
    pub(crate) existing_rollup: Option<LocalSubjectMemoryRecord>,
    pub(crate) candidates: Vec<LocalSubjectMemoryRecord>,
}

#[derive(Debug, Clone)]
pub(crate) struct SaveLocalSubjectMemoryRollupInput {
    pub(crate) owner_user_id: String,
    pub(crate) subject_type: String,
    pub(crate) subject_id: String,
    pub(crate) project_id: String,
    pub(crate) recall_text: String,
    pub(crate) source_session_id: String,
    pub(crate) source_summary_id: String,
    pub(crate) level: i64,
    pub(crate) candidate_ids: Vec<String>,
}
