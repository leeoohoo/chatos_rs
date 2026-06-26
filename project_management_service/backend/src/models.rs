use chrono::{SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub fn now_rfc3339() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true)
}

pub fn normalized_optional(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

pub fn validate_required(field: &str, value: &str) -> Result<(), String> {
    if value.trim().is_empty() {
        Err(format!("{field} is required"))
    } else {
        Ok(())
    }
}

pub fn work_item_requires_technical_overview_message() -> String {
    "创建项目任务前，请先填写该需求的实现技术总体文档内容".to_string()
}

pub trait DbStatus: Sized {
    fn as_str(&self) -> &'static str;
    fn from_db(value: &str) -> Self;
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum UserRole {
    Admin,
    Agent,
}

impl Default for UserRole {
    fn default() -> Self {
        Self::Agent
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthUser {
    pub id: String,
    pub username: String,
    pub display_name: String,
    pub role: UserRole,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoginResponse {
    pub token: String,
    pub user: AuthUser,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AgentTokenRequest {
    pub agent_account_id: Option<String>,
    pub contact_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentTokenResponse {
    pub access_token: String,
    pub token_type: String,
    pub expires_in: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentAccountListItem {
    pub id: String,
    pub username: String,
    pub display_name: String,
    pub owner_user_id: String,
    pub owner_username: String,
    pub owner_display_name: String,
    pub enabled: bool,
    pub created_at: String,
    pub updated_at: String,
    pub last_login_at: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProjectStatus {
    Active,
    Archived,
}

impl Default for ProjectStatus {
    fn default() -> Self {
        Self::Active
    }
}

impl DbStatus for ProjectStatus {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Archived => "archived",
        }
    }

    fn from_db(value: &str) -> Self {
        match value.trim() {
            "archived" => Self::Archived,
            _ => Self::Active,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectRecord {
    pub id: String,
    #[serde(default)]
    pub creator_user_id: Option<String>,
    #[serde(default)]
    pub creator_username: Option<String>,
    #[serde(default)]
    pub creator_display_name: Option<String>,
    pub owner_user_id: Option<String>,
    pub owner_username: Option<String>,
    pub owner_display_name: Option<String>,
    pub name: String,
    pub root_path: Option<String>,
    pub git_url: Option<String>,
    pub description: Option<String>,
    pub status: ProjectStatus,
    pub created_at: String,
    pub updated_at: String,
    pub archived_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateProjectRequest {
    pub name: String,
    pub root_path: Option<String>,
    pub git_url: Option<String>,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportProjectRequest {
    pub id: String,
    pub owner_user_id: Option<String>,
    pub owner_username: Option<String>,
    pub owner_display_name: Option<String>,
    pub name: String,
    pub root_path: Option<String>,
    pub git_url: Option<String>,
    pub description: Option<String>,
    pub status: Option<ProjectStatus>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
    pub archived_at: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UpdateProjectRequest {
    pub name: Option<String>,
    pub root_path: Option<String>,
    pub git_url: Option<String>,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectProfileRecord {
    pub project_id: String,
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
    pub background: Option<String>,
    pub introduction: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UpsertProjectProfileRequest {
    pub background: Option<String>,
    pub introduction: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RequirementStatus {
    Draft,
    Reviewing,
    Approved,
    InProgress,
    Done,
    Cancelled,
    Archived,
}

impl Default for RequirementStatus {
    fn default() -> Self {
        Self::Draft
    }
}

impl DbStatus for RequirementStatus {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::Reviewing => "reviewing",
            Self::Approved => "approved",
            Self::InProgress => "in_progress",
            Self::Done => "done",
            Self::Cancelled => "cancelled",
            Self::Archived => "archived",
        }
    }

    fn from_db(value: &str) -> Self {
        match value.trim() {
            "reviewing" => Self::Reviewing,
            "approved" => Self::Approved,
            "in_progress" => Self::InProgress,
            "done" => Self::Done,
            "cancelled" => Self::Cancelled,
            "archived" => Self::Archived,
            _ => Self::Draft,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RequirementType {
    Requirement,
    Change,
    BugFix,
}

impl Default for RequirementType {
    fn default() -> Self {
        Self::Requirement
    }
}

impl DbStatus for RequirementType {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Requirement => "requirement",
            Self::Change => "change",
            Self::BugFix => "bug_fix",
        }
    }

    fn from_db(value: &str) -> Self {
        match value.trim() {
            "change" => Self::Change,
            "bug_fix" => Self::BugFix,
            _ => Self::Requirement,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequirementRecord {
    pub id: String,
    pub project_id: String,
    pub parent_requirement_id: Option<String>,
    #[serde(default)]
    pub requirement_type: RequirementType,
    pub title: String,
    pub summary: Option<String>,
    pub detail: Option<String>,
    pub business_value: Option<String>,
    pub acceptance_criteria: Option<String>,
    pub source: Option<String>,
    pub priority: i64,
    pub status: RequirementStatus,
    #[serde(default)]
    pub creator_user_id: Option<String>,
    #[serde(default)]
    pub creator_username: Option<String>,
    #[serde(default)]
    pub creator_display_name: Option<String>,
    pub owner_user_id: Option<String>,
    #[serde(default)]
    pub owner_username: Option<String>,
    #[serde(default)]
    pub owner_display_name: Option<String>,
    pub assignee_user_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub archived_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateRequirementRequest {
    pub parent_requirement_id: Option<String>,
    pub requirement_type: Option<RequirementType>,
    pub title: String,
    pub summary: Option<String>,
    pub detail: Option<String>,
    pub business_value: Option<String>,
    pub acceptance_criteria: Option<String>,
    pub source: Option<String>,
    pub priority: Option<i64>,
    pub status: Option<RequirementStatus>,
    pub assignee_user_id: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UpdateRequirementRequest {
    pub parent_requirement_id: Option<String>,
    pub requirement_type: Option<RequirementType>,
    pub title: Option<String>,
    pub summary: Option<String>,
    pub detail: Option<String>,
    pub business_value: Option<String>,
    pub acceptance_criteria: Option<String>,
    pub source: Option<String>,
    pub priority: Option<i64>,
    pub status: Option<RequirementStatus>,
    pub assignee_user_id: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SetRequirementDependenciesRequest {
    pub prerequisite_requirement_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequirementDependencyRecord {
    pub requirement_id: String,
    pub prerequisite_requirement_id: String,
    pub relation_type: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequirementDocumentRecord {
    pub id: String,
    pub requirement_id: String,
    pub doc_type: String,
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
    pub title: String,
    pub format: String,
    pub content: String,
    pub version: i64,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpsertRequirementDocumentRequest {
    pub title: Option<String>,
    pub format: Option<String>,
    pub content: String,
}

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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DependencyGraphNode {
    pub id: String,
    pub node_type: String,
    pub label: String,
    pub status: String,
    pub parent_id: Option<String>,
    pub raw_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DependencyGraphEdge {
    pub from: String,
    pub to: String,
    pub edge_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DependencyGraphResponse {
    pub root_id: Option<String>,
    pub nodes: Vec<DependencyGraphNode>,
    pub edges: Vec<DependencyGraphEdge>,
    pub blocked_by: Vec<DependencyGraphNode>,
    pub ready: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskRunnerExecutionOptionRecord {
    pub id: String,
    pub label: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskRunnerExecutionOptionsResponse {
    pub model_configs: Vec<TaskRunnerExecutionOptionRecord>,
    pub tools: Vec<TaskRunnerExecutionOptionRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthResponse {
    pub ok: bool,
    pub service: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorResponse {
    pub error: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<Value>,
}
