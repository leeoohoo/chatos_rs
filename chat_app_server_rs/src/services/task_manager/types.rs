use serde::{Deserialize, Serialize};

use crate::services::task_service_client::{
    TaskContextAssetRefDto, TaskExecutionResultContractDto,
};

use super::normalizer::{normalize_priority, normalize_status, normalize_tags, trimmed_non_empty};

pub const REVIEW_TIMEOUT_MS_DEFAULT: u64 = 86_400_000;
pub const REVIEW_TIMEOUT_ERR: &str = "review_timeout";
pub const REVIEW_NOT_FOUND_ERR: &str = "review_not_found";
pub const TASK_NOT_FOUND_ERR: &str = "task_not_found";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskRequiredContextAssetDraft {
    pub asset_type: String,
    pub asset_ref: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskDraft {
    pub title: String,
    #[serde(default)]
    pub details: String,
    #[serde(default)]
    pub task_ref: Option<String>,
    #[serde(default)]
    pub task_kind: Option<String>,
    #[serde(default)]
    pub depends_on_refs: Vec<String>,
    #[serde(default)]
    pub verification_of_refs: Vec<String>,
    #[serde(default)]
    pub acceptance_criteria: Vec<String>,
    #[serde(default = "default_priority")]
    pub priority: String,
    #[serde(default = "default_status")]
    pub status: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub due_at: Option<String>,
    #[serde(default)]
    pub required_builtin_capabilities: Vec<String>,
    #[serde(default)]
    pub required_context_assets: Vec<TaskRequiredContextAssetDraft>,
    #[serde(default)]
    pub planned_builtin_mcp_ids: Vec<String>,
    #[serde(default)]
    pub planned_context_assets: Vec<TaskContextAssetRefDto>,
    #[serde(default)]
    pub execution_result_contract: Option<TaskExecutionResultContractDto>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TaskUpdatePatch {
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub details: Option<String>,
    #[serde(default)]
    pub priority: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub tags: Option<Vec<String>>,
    #[serde(default)]
    pub due_at: Option<Option<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskRecord {
    pub id: String,
    pub task_plan_id: Option<String>,
    pub task_ref: Option<String>,
    pub task_kind: Option<String>,
    #[serde(default)]
    pub depends_on_task_ids: Vec<String>,
    #[serde(default)]
    pub verification_of_task_ids: Vec<String>,
    #[serde(default)]
    pub acceptance_criteria: Vec<String>,
    pub blocked_reason: Option<String>,
    pub session_id: String,
    pub conversation_turn_id: String,
    pub project_root: Option<String>,
    pub remote_connection_id: Option<String>,
    pub title: String,
    pub details: String,
    pub priority: String,
    pub status: String,
    pub tags: Vec<String>,
    pub due_at: Option<String>,
    #[serde(default)]
    pub planned_builtin_mcp_ids: Vec<String>,
    #[serde(default)]
    pub planned_context_assets: Vec<TaskContextAssetRefDto>,
    pub execution_result_contract: Option<TaskExecutionResultContractDto>,
    pub planning_snapshot: Option<crate::services::task_service_client::TaskPlanningSnapshotDto>,
    pub result_summary: Option<String>,
    pub last_error: Option<String>,
    pub confirmed_at: Option<String>,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub task_result_brief: Option<TaskResultBrief>,
    pub handoff_payload: Option<TaskHandoffPayload>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskResultBrief {
    pub task_id: String,
    pub task_status: String,
    pub result_summary: String,
    pub result_format: Option<String>,
    pub result_message_id: Option<String>,
    pub source_session_id: Option<String>,
    pub source_turn_id: Option<String>,
    pub finished_at: Option<String>,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskHandoffPayload {
    pub task_id: String,
    pub task_plan_id: Option<String>,
    pub handoff_kind: String,
    pub summary: String,
    #[serde(default)]
    pub result_summary: Option<String>,
    #[serde(default)]
    pub key_changes: Vec<String>,
    #[serde(default)]
    pub changed_files: Vec<String>,
    #[serde(default)]
    pub executed_commands: Vec<String>,
    #[serde(default)]
    pub verification_suggestions: Vec<String>,
    #[serde(default)]
    pub open_risks: Vec<String>,
    #[serde(default)]
    pub artifact_refs: Vec<String>,
    #[serde(default)]
    pub checkpoint_message_ids: Vec<String>,
    #[serde(default)]
    pub result_brief_id: Option<String>,
    pub generated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskCreateReviewPayload {
    pub review_id: String,
    pub session_id: String,
    pub conversation_turn_id: String,
    pub draft_tasks: Vec<TaskDraft>,
    pub timeout_ms: u64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum TaskReviewAction {
    Confirm,
    Cancel,
}

impl TaskReviewAction {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Confirm => "confirm",
            Self::Cancel => "cancel",
        }
    }
}

#[derive(Debug, Clone)]
pub struct TaskReviewDecision {
    pub action: TaskReviewAction,
    pub tasks: Vec<TaskDraft>,
    pub reason: Option<String>,
}

impl TaskUpdatePatch {
    pub(super) fn is_empty(&self) -> bool {
        self.title.is_none()
            && self.details.is_none()
            && self.priority.is_none()
            && self.status.is_none()
            && self.tags.is_none()
            && self.due_at.is_none()
    }

    pub(super) fn normalized(mut self) -> Result<Self, String> {
        if let Some(title) = self.title.take() {
            let title = title.trim().to_string();
            if title.is_empty() {
                return Err("task title is required".to_string());
            }
            self.title = Some(title);
        }

        if let Some(details) = self.details.take() {
            self.details = Some(details.trim().to_string());
        }

        if let Some(priority) = self.priority.take() {
            self.priority = Some(normalize_priority(priority.as_str()));
        }

        if let Some(status) = self.status.take() {
            self.status = Some(normalize_status(status.as_str()));
        }

        if let Some(tags) = self.tags.take() {
            self.tags = Some(normalize_tags(tags));
        }

        if let Some(due_at) = self.due_at.take() {
            let normalized = due_at
                .as_deref()
                .and_then(trimmed_non_empty)
                .map(|value| value.to_string());
            self.due_at = Some(normalized);
        }

        Ok(self)
    }
}

fn default_priority() -> String {
    "medium".to_string()
}

fn default_status() -> String {
    "pending_confirm".to_string()
}
