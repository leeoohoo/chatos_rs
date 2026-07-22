// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_mcp::{AskUserPromptPayload, AskUserResponseSubmission};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use super::{default_true, now_rfc3339};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum TaskRunStatus {
    #[default]
    Queued,
    Running,
    Succeeded,
    Failed,
    Cancelled,
    Blocked,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ChatosCallbackDeliveryStatus {
    Pending,
    Delivered,
    Skipped,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ChatosCallbackDeliveryState {
    pub event: String,
    pub status: ChatosCallbackDeliveryStatus,
    #[serde(default)]
    pub attempt_count: u32,
    #[serde(default)]
    pub next_attempt_at: Option<String>,
    #[serde(default)]
    pub last_error: Option<String>,
    pub updated_at: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum AskUserPromptStatus {
    #[default]
    Pending,
    Submitted,
    Cancelled,
    TimedOut,
    Failed,
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
    #[serde(default)]
    pub chatos_callback_delivery: Option<ChatosCallbackDeliveryState>,
    pub created_at: String,
    pub updated_at: String,
}

impl TaskRunRecord {
    pub fn queued(
        id: String,
        task_id: String,
        model_config_id: String,
        memory_thread_id: String,
        input_snapshot: Value,
        now: String,
    ) -> Self {
        Self {
            id,
            task_id,
            model_config_id,
            memory_thread_id,
            status: TaskRunStatus::Queued,
            started_at: None,
            finished_at: None,
            input_snapshot,
            context_snapshot: None,
            result_summary: None,
            error_message: None,
            usage: None,
            report: None,
            cancel_requested: false,
            summary_job_run_id: None,
            worker_id: None,
            claim_token: None,
            claim_until: None,
            attempt: 0,
            chatos_callback_delivery: None,
            created_at: now.clone(),
            updated_at: now,
        }
    }
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
