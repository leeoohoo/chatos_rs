use serde::{Deserialize, Serialize};

use super::requirements::{RequirementRecord, RequirementStatus};
use super::DbStatus;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProjectWorkItemStatus {
    Todo,
    Ready,
    InProgress,
    Blocked,
    Done,
    Cancelled,
    Archived,
}

impl Default for ProjectWorkItemStatus {
    fn default() -> Self {
        Self::Todo
    }
}

impl DbStatus for ProjectWorkItemStatus {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Todo => "todo",
            Self::Ready => "ready",
            Self::InProgress => "in_progress",
            Self::Blocked => "blocked",
            Self::Done => "done",
            Self::Cancelled => "cancelled",
            Self::Archived => "archived",
        }
    }

    fn from_db(value: &str) -> Self {
        match value.trim() {
            "ready" => Self::Ready,
            "in_progress" => Self::InProgress,
            "blocked" => Self::Blocked,
            "done" => Self::Done,
            "cancelled" => Self::Cancelled,
            "archived" => Self::Archived,
            _ => Self::Todo,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectWorkItemRecord {
    pub id: String,
    pub project_id: String,
    pub requirement_id: String,
    pub title: String,
    pub description: Option<String>,
    #[serde(default)]
    pub task_runner_default_model_config_id: String,
    #[serde(default)]
    pub task_runner_enabled_tool_ids: Vec<String>,
    pub status: ProjectWorkItemStatus,
    pub priority: i64,
    pub assignee_user_id: Option<String>,
    pub estimate_points: Option<i64>,
    pub due_at: Option<String>,
    pub sort_order: i64,
    pub tags: Vec<String>,
    #[serde(default)]
    pub creator_user_id: Option<String>,
    #[serde(default)]
    pub creator_username: Option<String>,
    #[serde(default)]
    pub creator_display_name: Option<String>,
    #[serde(default)]
    pub owner_user_id: Option<String>,
    #[serde(default)]
    pub owner_username: Option<String>,
    #[serde(default)]
    pub owner_display_name: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub archived_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateProjectWorkItemRequest {
    pub title: String,
    pub description: Option<String>,
    pub task_runner_default_model_config_id: String,
    pub task_runner_enabled_tool_ids: Vec<String>,
    pub status: Option<ProjectWorkItemStatus>,
    pub priority: Option<i64>,
    pub assignee_user_id: Option<String>,
    pub estimate_points: Option<i64>,
    pub due_at: Option<String>,
    pub sort_order: Option<i64>,
    pub tags: Option<Vec<String>>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UpdateProjectWorkItemRequest {
    pub requirement_id: Option<String>,
    pub title: Option<String>,
    pub description: Option<String>,
    pub status: Option<ProjectWorkItemStatus>,
    pub priority: Option<i64>,
    pub assignee_user_id: Option<String>,
    pub estimate_points: Option<i64>,
    pub due_at: Option<String>,
    pub sort_order: Option<i64>,
    pub tags: Option<Vec<String>>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SetWorkItemDependenciesRequest {
    pub prerequisite_work_item_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkItemDependencyRecord {
    pub work_item_id: String,
    pub prerequisite_work_item_id: String,
    pub relation_type: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectWorkItemTaskRunnerLinkRecord {
    pub id: String,
    pub work_item_id: String,
    pub task_runner_task_id: String,
    pub task_runner_run_id: Option<String>,
    pub link_type: String,
    #[serde(default)]
    pub source_session_id: Option<String>,
    #[serde(default)]
    pub source_user_message_id: Option<String>,
    #[serde(default)]
    pub task_runner_status: Option<String>,
    #[serde(default)]
    pub last_callback_event: Option<String>,
    #[serde(default)]
    pub last_callback_at: Option<String>,
    #[serde(default)]
    pub last_error_message: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LinkTaskRunnerTaskRequest {
    pub task_runner_task_id: String,
    pub task_runner_run_id: Option<String>,
    pub link_type: Option<String>,
    pub source_session_id: Option<String>,
    pub source_user_message_id: Option<String>,
    pub task_runner_status: Option<String>,
    pub last_callback_event: Option<String>,
    pub last_callback_at: Option<String>,
    pub last_error_message: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CreateTaskRunnerTaskFromWorkItemRequest {
    pub title: Option<String>,
    pub description: Option<String>,
    pub objective: Option<String>,
    pub priority: Option<i32>,
    pub tags: Option<Vec<String>>,
    pub default_model_config_id: Option<String>,
    pub prerequisite_task_ids: Option<Vec<String>>,
    pub source_session_id: Option<String>,
    pub source_user_message_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskRunnerTaskRecord {
    pub id: String,
    pub title: String,
    pub status: String,
    pub project_id: String,
    pub last_run_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateTaskRunnerTaskFromWorkItemResponse {
    pub task: TaskRunnerTaskRecord,
    pub link: ProjectWorkItemTaskRunnerLinkRecord,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SyncTaskRunnerWorkItemStatusRequest {
    pub task_runner_task_id: String,
    pub task_runner_run_id: Option<String>,
    pub task_runner_status: Option<String>,
    pub last_callback_event: Option<String>,
    pub last_callback_at: Option<String>,
    pub last_error_message: Option<String>,
    pub source_session_id: Option<String>,
    pub source_user_message_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncTaskRunnerWorkItemStatusResponse {
    pub work_item: ProjectWorkItemRecord,
    pub link: ProjectWorkItemTaskRunnerLinkRecord,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SyncRequirementExecutionStateRequest {
    pub requirement_status: Option<RequirementStatus>,
    #[serde(default)]
    pub work_item_ids: Vec<String>,
    pub work_item_status: Option<ProjectWorkItemStatus>,
    #[serde(default)]
    pub skip_done_work_items: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncRequirementExecutionStateResponse {
    pub requirement: RequirementRecord,
    pub work_items: Vec<ProjectWorkItemRecord>,
}
