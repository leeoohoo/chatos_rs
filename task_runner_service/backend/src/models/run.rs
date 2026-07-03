// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_builtin_tools::{AskUserPromptPayload, AskUserResponseSubmission};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use super::{default_true, now_rfc3339};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TaskRunStatus {
    Queued,
    Running,
    Succeeded,
    Failed,
    Cancelled,
    Blocked,
}

impl Default for TaskRunStatus {
    fn default() -> Self {
        Self::Queued
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AskUserPromptStatus {
    Pending,
    Submitted,
    Cancelled,
    TimedOut,
    Failed,
}

impl Default for AskUserPromptStatus {
    fn default() -> Self {
        Self::Pending
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskRunRecord {
    pub id: String,
    pub task_id: String,
    pub model_config_id: String,
    pub memory_thread_id: String,
    pub status: TaskRunStatus,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
    pub input_snapshot: Value,
    pub context_snapshot: Option<Value>,
    pub result_summary: Option<String>,
    pub error_message: Option<String>,
    pub usage: Option<Value>,
    pub report: Option<Value>,
    pub cancel_requested: bool,
    pub summary_job_run_id: Option<String>,
    #[serde(default)]
    pub worker_id: Option<String>,
    #[serde(default)]
    pub claim_token: Option<String>,
    #[serde(default)]
    pub claim_until: Option<String>,
    #[serde(default)]
    pub attempt: i64,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskRunEventRecord {
    pub id: String,
    pub run_id: String,
    pub event_type: String,
    pub message: Option<String>,
    pub payload: Option<Value>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AskUserPromptRecord {
    pub id: String,
    #[serde(default)]
    pub task_id: Option<String>,
    #[serde(default)]
    pub run_id: Option<String>,
    pub conversation_id: String,
    pub conversation_turn_id: String,
    #[serde(default)]
    pub tool_call_id: Option<String>,
    pub kind: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub message: String,
    #[serde(default = "default_true")]
    pub allow_cancel: bool,
    pub timeout_ms: u64,
    pub payload: Value,
    #[serde(default)]
    pub response: Option<AskUserResponseSubmission>,
    pub status: AskUserPromptStatus,
    pub created_at: String,
    pub updated_at: String,
    #[serde(default)]
    pub expires_at: Option<String>,
}

impl AskUserPromptRecord {
    pub fn from_payload(
        payload: AskUserPromptPayload,
        task_id: Option<String>,
        run_id: Option<String>,
        created_at: String,
        expires_at: Option<String>,
    ) -> Self {
        Self {
            id: payload.prompt_id,
            task_id,
            run_id,
            conversation_id: payload.conversation_id,
            conversation_turn_id: payload.conversation_turn_id,
            tool_call_id: payload.tool_call_id,
            kind: payload.kind,
            title: payload.title,
            message: payload.message,
            allow_cancel: payload.allow_cancel,
            timeout_ms: payload.timeout_ms,
            payload: payload.payload,
            response: None,
            status: AskUserPromptStatus::Pending,
            created_at: created_at.clone(),
            updated_at: created_at,
            expires_at,
        }
    }
}

impl TaskRunEventRecord {
    pub fn new(
        run_id: impl Into<String>,
        event_type: impl Into<String>,
        message: Option<String>,
        payload: Option<Value>,
    ) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            run_id: run_id.into(),
            event_type: event_type.into(),
            message,
            payload,
            created_at: now_rfc3339(),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RunListFilters {
    pub task_id: Option<String>,
    pub status: Option<TaskRunStatus>,
    pub model_config_id: Option<String>,
    pub keyword: Option<String>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PromptListFilters {
    pub task_id: Option<String>,
    pub run_id: Option<String>,
    pub status: Option<AskUserPromptStatus>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunSummaryRecord {
    pub id: String,
    pub task_id: String,
    pub status: TaskRunStatus,
    pub model_config_id: String,
    pub updated_at: String,
}

impl From<&TaskRunRecord> for RunSummaryRecord {
    fn from(value: &TaskRunRecord) -> Self {
        Self {
            id: value.id.clone(),
            task_id: value.task_id.clone(),
            status: value.status,
            model_config_id: value.model_config_id.clone(),
            updated_at: value.updated_at.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AskUserPromptTaskCountRecord {
    pub task_id: String,
    pub count: usize,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StartTaskRunRequest {
    pub model_config_id: Option<String>,
    pub prompt_override: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SubmitAskUserPromptRequest {
    pub values: Option<Value>,
    pub selection: Option<Value>,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CancelAskUserPromptRequest {
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RunOutputFileChangeCounts {
    #[serde(default)]
    pub added: usize,
    #[serde(default)]
    pub modified: usize,
    #[serde(default)]
    pub deleted: usize,
    #[serde(default)]
    pub binary: usize,
    #[serde(default)]
    pub diff_available: usize,
    #[serde(default)]
    pub total: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunOutputFileChange {
    pub path: String,
    pub status: String,
    #[serde(default)]
    pub old_size: Option<u64>,
    #[serde(default)]
    pub new_size: Option<u64>,
    #[serde(default)]
    pub old_sha256: Option<String>,
    #[serde(default)]
    pub new_sha256: Option<String>,
    #[serde(default)]
    pub added_lines: usize,
    #[serde(default)]
    pub deleted_lines: usize,
    #[serde(default)]
    pub binary: bool,
    #[serde(default)]
    pub diff_available: bool,
    #[serde(default)]
    pub diff_truncated: bool,
    #[serde(default)]
    pub diff_ref: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunOutputChangeManifest {
    pub schema_version: u32,
    pub run_id: String,
    pub sandbox_id: String,
    pub lease_id: String,
    pub generated_at: String,
    #[serde(default)]
    pub output_workspace: Option<String>,
    #[serde(default)]
    pub manifest_path: Option<String>,
    #[serde(default)]
    pub counts: RunOutputFileChangeCounts,
    #[serde(default)]
    pub files: Vec<RunOutputFileChange>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunOutputChangesResponse {
    pub run_id: String,
    pub counts: RunOutputFileChangeCounts,
    pub files: Vec<RunOutputFileChange>,
    pub total: usize,
    pub limit: usize,
    pub offset: usize,
    pub has_more: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunOutputDiffResponse {
    pub run_id: String,
    pub path: String,
    pub status: String,
    #[serde(default)]
    pub patch: Option<String>,
    #[serde(default)]
    pub binary: bool,
    #[serde(default)]
    pub diff_available: bool,
    #[serde(default)]
    pub diff_truncated: bool,
    #[serde(default)]
    pub message: Option<String>,
}
