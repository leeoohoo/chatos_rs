use serde::Deserialize;
use serde_json::Value;

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
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

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RequirementType {
    Requirement,
    Change,
    BugFix,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProjectTaskStatus {
    Todo,
    Ready,
    InProgress,
    Blocked,
    Done,
    Cancelled,
    Archived,
}

#[derive(Debug, Deserialize)]
pub struct ToolCallParams {
    pub name: String,
    #[serde(default)]
    pub arguments: Value,
}

#[derive(Debug, Deserialize)]
pub struct RequirementIdArgs {
    pub requirement_id: String,
}

#[derive(Debug, Deserialize)]
pub struct InitProjectArgs {
    pub name: Option<String>,
    pub root_path: Option<String>,
    pub git_url: Option<String>,
    pub description: Option<String>,
    pub background: Option<String>,
    pub introduction: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CreateRequirementArgs {
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

#[derive(Debug, Deserialize)]
pub struct UpdateRequirementArgs {
    pub requirement_id: String,
    pub patch: UpdateRequirementPatch,
    pub prerequisite_requirement_ids: Option<Vec<String>>,
}

#[derive(Debug, Default, Deserialize)]
pub struct UpdateRequirementPatch {
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

#[derive(Debug, Deserialize)]
pub struct ProjectTaskIdArgs {
    pub project_task_id: String,
}

#[derive(Debug, Deserialize)]
pub struct ListRequirementsArgs {
    pub status: Option<RequirementStatus>,
    pub keyword: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ListProjectTasksArgs {
    pub status: Option<ProjectTaskStatus>,
    pub keyword: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CreateProjectTaskArgs {
    pub requirement_id: String,
    pub title: String,
    pub description: Option<String>,
    pub task_runner_default_model_config_id: String,
    pub task_runner_enabled_tool_ids: Vec<String>,
    pub status: Option<ProjectTaskStatus>,
    pub priority: Option<i64>,
    pub assignee_user_id: Option<String>,
    pub estimate_points: Option<i64>,
    pub due_at: Option<String>,
    pub sort_order: Option<i64>,
    pub tags: Option<Vec<String>>,
    pub prerequisite_project_task_ids: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateProjectTaskArgs {
    pub project_task_id: String,
    pub patch: UpdateProjectTaskPatch,
    pub prerequisite_project_task_ids: Option<Vec<String>>,
}

#[derive(Debug, Default, Deserialize)]
pub struct UpdateProjectTaskPatch {
    pub requirement_id: Option<String>,
    pub title: Option<String>,
    pub description: Option<String>,
    pub status: Option<ProjectTaskStatus>,
    pub priority: Option<i64>,
    pub assignee_user_id: Option<String>,
    pub estimate_points: Option<i64>,
    pub due_at: Option<String>,
    pub sort_order: Option<i64>,
    pub tags: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
pub struct SetRequirementDependenciesArgs {
    pub requirement_id: String,
    pub prerequisite_requirement_ids: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct SetProjectTaskDependenciesArgs {
    pub project_task_id: String,
    pub prerequisite_project_task_ids: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpsertTechnicalOverviewArgs {
    pub requirement_id: String,
    pub title: Option<String>,
    pub format: Option<String>,
    pub content: String,
}
