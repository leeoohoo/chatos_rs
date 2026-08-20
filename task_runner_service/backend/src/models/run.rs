// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_mcp::{AskUserPromptPayload, AskUserResponseSubmission};
use chatos_mcp_management_sdk::RuntimeWorkspaceRouteTarget;
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
pub enum TaskRunAttemptStatus {
    Running,
    Succeeded,
    Failed,
    Cancelled,
    Blocked,
    Interrupted,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TaskRunAttemptRecord {
    pub attempt_id: String,
    pub sequence: i64,
    pub status: TaskRunAttemptStatus,
    pub started_at: String,
    #[serde(default)]
    pub finished_at: Option<String>,
    #[serde(default)]
    pub recovery_reason: Option<String>,
    #[serde(default)]
    pub model_response_id: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ChatosCallbackDeliveryStatus {
    Pending,
    Enqueued,
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct EffectiveTaskToolSnapshot {
    #[serde(default)]
    pub requested_mcp_resource_ids: Vec<String>,
    #[serde(default)]
    pub workspace_read: bool,
    #[serde(default)]
    pub workspace_write: bool,
    #[serde(default)]
    pub terminal: bool,
}

impl EffectiveTaskToolSnapshot {
    pub fn mutates_workspace(&self) -> bool {
        self.workspace_write || self.terminal
    }

    pub fn uses_workspace(&self) -> bool {
        self.workspace_read || self.workspace_write || self.terminal
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WorkspacePreparationStatus {
    Pending,
    Ready,
    Failed,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum ModelPhaseStatus {
    #[default]
    Pending,
    Running,
    Succeeded,
    Failed,
    Cancelled,
    Blocked,
}

impl ModelPhaseStatus {
    pub const fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Succeeded | Self::Failed | Self::Cancelled | Self::Blocked
        )
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum WorkspaceIntegrationStatus {
    #[default]
    NotRequired,
    Pending,
    Integrating,
    Integrated,
    Waived,
    Conflict,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TaskRunBranchTarget {
    Local,
    Default {
        branch_ref: String,
    },
    Run {
        branch_id: String,
        branch_ref: String,
        base_branch: String,
        base_commit: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TaskRunWorkspaceChangedFile {
    pub status: String,
    pub path: String,
    pub old_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TaskRunWorkspaceExecution {
    pub status: WorkspacePreparationStatus,
    #[serde(default)]
    pub route: Option<RuntimeWorkspaceRouteTarget>,
    #[serde(default)]
    pub branch_target: Option<TaskRunBranchTarget>,
    #[serde(default)]
    pub execution_group_id: Option<String>,
    #[serde(default)]
    pub execution_branch_ref: Option<String>,
    #[serde(default)]
    pub execution_base_commit: Option<String>,
    #[serde(default)]
    pub integration_status: WorkspaceIntegrationStatus,
    #[serde(default)]
    pub integration_ready_at: Option<String>,
    #[serde(default)]
    pub integration_started_at: Option<String>,
    #[serde(default)]
    pub integrated_at: Option<String>,
    #[serde(default)]
    pub integration_attempt_count: u32,
    #[serde(default)]
    pub integration_base_commit: Option<String>,
    #[serde(default)]
    pub result_commit: Option<String>,
    #[serde(default)]
    pub integrated_commit: Option<String>,
    #[serde(default)]
    pub promoted_commit: Option<String>,
    #[serde(default)]
    pub waived_at: Option<String>,
    #[serde(default)]
    pub waiver_reason: Option<String>,
    #[serde(default)]
    pub local_changed_files: Vec<TaskRunWorkspaceChangedFile>,
    #[serde(default)]
    pub local_patch: Option<String>,
    #[serde(default)]
    pub local_patch_truncated: bool,
    #[serde(default)]
    pub conflict_files: Vec<String>,
    #[serde(default)]
    pub conflict_message: Option<String>,
    #[serde(default)]
    pub integration_last_error: Option<String>,
    #[serde(default)]
    pub prepared_at: Option<String>,
    #[serde(default)]
    pub finalized_at: Option<String>,
    #[serde(default)]
    pub lease_retained_for_diagnostics: bool,
    #[serde(default)]
    pub finalization_error: Option<String>,
    #[serde(default)]
    pub error: Option<String>,
}

impl TaskRunWorkspaceExecution {
    pub fn integration_requires_post_process(&self) -> bool {
        matches!(
            self.integration_status,
            WorkspaceIntegrationStatus::Pending
                | WorkspaceIntegrationStatus::Integrating
                | WorkspaceIntegrationStatus::Failed
        )
    }

    pub fn integration_satisfied(&self) -> bool {
        matches!(
            self.integration_status,
            WorkspaceIntegrationStatus::NotRequired
                | WorkspaceIntegrationStatus::Integrated
                | WorkspaceIntegrationStatus::Waived
        )
    }
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_run_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_ordering_lane_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_lane_seq: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub execution_lane_key: Option<String>,
    pub model_config_id: String,
    pub memory_thread_id: String,
    pub status: TaskRunStatus,
    #[serde(default)]
    pub model_phase_status: ModelPhaseStatus,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
    pub input_snapshot: Value,
    #[serde(default)]
    pub effective_tools: EffectiveTaskToolSnapshot,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace_execution: Option<TaskRunWorkspaceExecution>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mcp_runtime_session_ref: Option<String>,
    pub context_snapshot: Option<Value>,
    pub result_summary: Option<String>,
    pub error_message: Option<String>,
    pub usage: Option<Value>,
    pub report: Option<Value>,
    pub cancel_requested: bool,
    #[serde(default)]
    pub cancel_event_pending: bool,
    #[serde(default)]
    pub dispatch_paused: bool,
    #[serde(default)]
    pub dispatch_event_pending: bool,
    #[serde(default)]
    pub post_process_event_pending: bool,
    #[serde(default)]
    pub post_process_event_enqueued: bool,
    #[serde(default)]
    pub post_process_completed: bool,
    #[serde(default)]
    pub post_process_dead_lettered: bool,
    #[serde(default)]
    pub post_process_attempt_count: u32,
    #[serde(default)]
    pub post_process_last_error: Option<String>,
    #[serde(default)]
    pub memory_summary_processed: bool,
    #[serde(default)]
    pub chatos_followup_processed: bool,
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
    pub attempts: Vec<TaskRunAttemptRecord>,
    #[serde(default)]
    pub chatos_started_callback_delivery: Option<ChatosCallbackDeliveryState>,
    #[serde(default)]
    pub chatos_callback_delivery: Option<ChatosCallbackDeliveryState>,
    pub created_at: String,
    pub updated_at: String,
}

impl TaskRunRecord {
    pub fn is_waiting_for_workspace_integration(&self) -> bool {
        self.status == TaskRunStatus::Running
            && self.model_phase_status.is_terminal()
            && self
                .workspace_execution
                .as_ref()
                .is_some_and(TaskRunWorkspaceExecution::integration_requires_post_process)
    }

    pub fn cancel_before_workspace_integration(&mut self, message: String, now: &str) {
        self.status = TaskRunStatus::Cancelled;
        self.model_phase_status = ModelPhaseStatus::Cancelled;
        self.cancel_requested = false;
        self.cancel_event_pending = false;
        self.claim_token = None;
        self.claim_until = None;
        self.finished_at = Some(now.to_string());
        self.updated_at = now.to_string();
        self.error_message = Some(message.clone());
        if let Some(execution) = self.workspace_execution.as_mut() {
            execution.integration_status = WorkspaceIntegrationStatus::NotRequired;
            execution.integration_started_at = None;
            execution.integration_last_error = Some(message);
        }
    }

    pub fn requires_post_process(&self) -> bool {
        self.model_phase_status.is_terminal()
            && (matches!(
                self.status,
                TaskRunStatus::Succeeded
                    | TaskRunStatus::Failed
                    | TaskRunStatus::Cancelled
                    | TaskRunStatus::Blocked
            ) || self
                .workspace_execution
                .as_ref()
                .is_some_and(TaskRunWorkspaceExecution::integration_requires_post_process))
    }

    pub fn queued(
        id: String,
        task_id: String,
        model_config_id: String,
        task_memory_thread_id: String,
        input_snapshot: Value,
        now: String,
    ) -> Self {
        let memory_thread_id =
            task_run_memory_thread_id(task_memory_thread_id.as_str(), id.as_str());
        Self {
            id,
            task_id,
            agent_run_id: None,
            agent_ordering_lane_key: None,
            agent_lane_seq: None,
            execution_lane_key: None,
            model_config_id,
            memory_thread_id,
            status: TaskRunStatus::Queued,
            model_phase_status: ModelPhaseStatus::Pending,
            started_at: None,
            finished_at: None,
            input_snapshot,
            effective_tools: EffectiveTaskToolSnapshot::default(),
            workspace_execution: None,
            mcp_runtime_session_ref: None,
            context_snapshot: None,
            result_summary: None,
            error_message: None,
            usage: None,
            report: None,
            cancel_requested: false,
            cancel_event_pending: false,
            dispatch_paused: false,
            dispatch_event_pending: true,
            post_process_event_pending: false,
            post_process_event_enqueued: false,
            post_process_completed: false,
            post_process_dead_lettered: false,
            post_process_attempt_count: 0,
            post_process_last_error: None,
            memory_summary_processed: false,
            chatos_followup_processed: false,
            summary_job_run_id: None,
            worker_id: None,
            claim_token: None,
            claim_until: None,
            attempt: 0,
            attempts: Vec::new(),
            chatos_started_callback_delivery: None,
            chatos_callback_delivery: None,
            created_at: now.clone(),
            updated_at: now,
        }
    }

    pub fn begin_attempt(&mut self, attempt_id: &str, started_at: &str) {
        if self
            .attempts
            .iter()
            .any(|attempt| attempt.attempt_id == attempt_id)
        {
            return;
        }
        for attempt in self
            .attempts
            .iter_mut()
            .filter(|attempt| attempt.status == TaskRunAttemptStatus::Running)
        {
            attempt.status = TaskRunAttemptStatus::Interrupted;
            attempt.finished_at = Some(started_at.to_string());
        }
        self.attempts.push(TaskRunAttemptRecord {
            attempt_id: attempt_id.to_string(),
            sequence: self.attempt,
            status: TaskRunAttemptStatus::Running,
            started_at: started_at.to_string(),
            finished_at: None,
            recovery_reason: (self.attempt > 1).then(|| "worker_claim_expired".to_string()),
            model_response_id: None,
        });
    }

    pub fn bind_current_attempt_model_response(&mut self, response_id: Option<&str>) {
        let Some(response_id) = response_id.map(str::trim).filter(|value| !value.is_empty()) else {
            return;
        };
        if let Some(attempt) = self
            .attempts
            .iter_mut()
            .rev()
            .find(|attempt| attempt.status == TaskRunAttemptStatus::Running)
        {
            attempt.model_response_id = Some(response_id.to_string());
        }
    }

    pub fn finish_current_attempt(&mut self, status: TaskRunAttemptStatus, finished_at: &str) {
        if let Some(attempt) = self
            .attempts
            .iter_mut()
            .rev()
            .find(|attempt| attempt.status == TaskRunAttemptStatus::Running)
        {
            attempt.status = status;
            attempt.finished_at = Some(finished_at.to_string());
        }
    }
}

pub fn task_run_memory_thread_id(task_memory_thread_id: &str, run_id: &str) -> String {
    let task_memory_thread_id = task_memory_thread_id.trim();
    let run_id = run_id.trim();
    let suffix = format!(":run:{run_id}");
    if task_memory_thread_id.ends_with(suffix.as_str()) {
        task_memory_thread_id.to_string()
    } else {
        format!("{task_memory_thread_id}{suffix}")
    }
}

#[cfg(test)]
mod tests {
    use super::{
        task_run_memory_thread_id, EffectiveTaskToolSnapshot, TaskRunAttemptStatus, TaskRunRecord,
    };
    use mongodb::bson;
    use serde_json::json;

    #[test]
    fn task_runs_receive_distinct_memory_threads() {
        assert_eq!(
            task_run_memory_thread_id("task-1", "run-1"),
            "task-1:run:run-1"
        );
        assert_eq!(
            task_run_memory_thread_id("task-1", "run-2"),
            "task-1:run:run-2"
        );
    }

    #[test]
    fn task_run_memory_thread_id_is_idempotent_for_the_same_run() {
        assert_eq!(
            task_run_memory_thread_id("task-1:run:run-1", "run-1"),
            "task-1:run:run-1"
        );
    }

    #[test]
    fn run_attempt_records_model_response_and_terminal_state() {
        let mut run = TaskRunRecord::queued(
            "run-1".to_string(),
            "task-1".to_string(),
            "model-1".to_string(),
            "thread-1".to_string(),
            json!({}),
            "2026-08-07T00:00:00Z".to_string(),
        );
        run.attempt = 1;
        run.begin_attempt("claim-1", "2026-08-07T00:01:00Z");
        run.bind_current_attempt_model_response(Some("response-1"));
        run.finish_current_attempt(TaskRunAttemptStatus::Succeeded, "2026-08-07T00:02:00Z");

        assert_eq!(run.attempts.len(), 1);
        let attempt = &run.attempts[0];
        assert_eq!(attempt.model_response_id.as_deref(), Some("response-1"));
        assert_eq!(attempt.status, TaskRunAttemptStatus::Succeeded);
        assert_eq!(attempt.finished_at.as_deref(), Some("2026-08-07T00:02:00Z"));
    }

    #[test]
    fn legacy_run_without_effective_tools_uses_empty_snapshot() {
        let run = TaskRunRecord::queued(
            "run-legacy".to_string(),
            "task-legacy".to_string(),
            "model-1".to_string(),
            "thread-1".to_string(),
            json!({}),
            "2026-08-07T00:00:00Z".to_string(),
        );
        let mut document = bson::to_document(&run).expect("serialize run as Mongo document");
        document.remove("effective_tools");

        let decoded: TaskRunRecord =
            bson::from_document(document).expect("decode legacy run without effective_tools");

        assert_eq!(
            decoded.effective_tools,
            EffectiveTaskToolSnapshot::default()
        );
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
    #[serde(default)]
    pub resolution_event_pending: bool,
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
            resolution_event_pending: false,
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
    pub retry_instruction: Option<String>,
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
