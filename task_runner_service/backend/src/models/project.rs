use serde::{Deserialize, Serialize};

pub const PUBLIC_PROJECT_ID: &str = "-1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskProjectStatus {
    Active,
    Archived,
}

impl Default for TaskProjectStatus {
    fn default() -> Self {
        Self::Active
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskProjectRecord {
    pub id: String,
    #[serde(default)]
    pub owner_user_id: Option<String>,
    #[serde(default)]
    pub owner_username: Option<String>,
    #[serde(default)]
    pub owner_display_name: Option<String>,
    pub name: String,
    #[serde(default)]
    pub root_path: Option<String>,
    #[serde(default)]
    pub git_url: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub status: TaskProjectStatus,
    pub created_at: String,
    pub updated_at: String,
    #[serde(default)]
    pub archived_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateTaskProjectRequest {
    pub name: String,
    pub root_path: Option<String>,
    pub git_url: Option<String>,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UpdateTaskProjectRequest {
    pub name: Option<String>,
    pub root_path: Option<String>,
    pub git_url: Option<String>,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatosProjectImportRequest {
    pub id: String,
    pub owner_user_id: Option<String>,
    pub owner_username: Option<String>,
    pub owner_display_name: Option<String>,
    pub name: String,
    pub root_path: Option<String>,
    pub git_url: Option<String>,
    pub description: Option<String>,
    pub status: Option<TaskProjectStatus>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
    pub archived_at: Option<String>,
}

pub fn normalize_project_id(value: Option<String>) -> String {
    value
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty() && *value != "0")
        .unwrap_or(PUBLIC_PROJECT_ID)
        .to_string()
}

pub fn task_project_status_to_str(status: TaskProjectStatus) -> &'static str {
    match status {
        TaskProjectStatus::Active => "active",
        TaskProjectStatus::Archived => "archived",
    }
}

pub fn task_project_status_from_str(value: &str) -> TaskProjectStatus {
    match value.trim().to_ascii_lowercase().as_str() {
        "archived" => TaskProjectStatus::Archived,
        _ => TaskProjectStatus::Active,
    }
}
