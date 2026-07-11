// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeMap;

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
    Failed,
    Done,
    Cancelled,
    Archived,
}

impl ProjectWorkItemStatus {
    pub const ALL: [Self; 8] = [
        Self::Todo,
        Self::Ready,
        Self::InProgress,
        Self::Blocked,
        Self::Failed,
        Self::Done,
        Self::Cancelled,
        Self::Archived,
    ];
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
            Self::Failed => "failed",
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
            "failed" => Self::Failed,
            "done" => Self::Done,
            "cancelled" => Self::Cancelled,
            "archived" => Self::Archived,
            _ => Self::Todo,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProjectWorkItemStatusCounts {
    pub total: i64,
    pub open: i64,
    pub done: i64,
    pub blocked: i64,
    pub failed: i64,
    pub by_status: BTreeMap<String, i64>,
}

impl ProjectWorkItemStatusCounts {
    pub fn add_status_count(&mut self, status: &str, count: i64) {
        if count <= 0 {
            return;
        }

        let status = ProjectWorkItemStatus::from_db(status);
        let key = status.as_str().to_string();
        *self.by_status.entry(key).or_default() += count;
        self.total += count;

        match status {
            ProjectWorkItemStatus::Done => {
                self.done += count;
            }
            ProjectWorkItemStatus::Blocked => {
                self.blocked += count;
                self.open += count;
            }
            ProjectWorkItemStatus::Failed => {
                self.failed += count;
                self.open += count;
            }
            _ => {
                self.open += count;
            }
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
    pub status: ProjectWorkItemStatus,
    pub priority: i64,
    pub assignee_user_id: Option<String>,
    pub estimate_points: Option<i64>,
    pub due_at: Option<String>,
    pub sort_order: i64,
    pub tags: Vec<String>,
    #[serde(default)]
    pub is_planning_task: bool,
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
    pub status: Option<ProjectWorkItemStatus>,
    pub priority: Option<i64>,
    pub assignee_user_id: Option<String>,
    pub estimate_points: Option<i64>,
    pub due_at: Option<String>,
    pub sort_order: Option<i64>,
    pub tags: Option<Vec<String>>,
    #[serde(default)]
    pub is_planning_task: bool,
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
    pub is_planning_task: Option<bool>,
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
    pub execution_group_id: Option<String>,
    #[serde(default = "default_true")]
    pub is_current: bool,
    #[serde(default)]
    pub superseded_at: Option<String>,
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
    pub execution_group_id: Option<String>,
    pub is_current: Option<bool>,
    pub superseded_at: Option<String>,
    pub source_session_id: Option<String>,
    pub source_user_message_id: Option<String>,
    pub task_runner_status: Option<String>,
    pub last_callback_event: Option<String>,
    pub last_callback_at: Option<String>,
    pub last_error_message: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SyncTaskRunnerWorkItemStatusRequest {
    pub task_runner_task_id: String,
    pub task_runner_run_id: Option<String>,
    pub task_runner_status: Option<String>,
    pub execution_group_id: Option<String>,
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

fn default_true() -> bool {
    true
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
