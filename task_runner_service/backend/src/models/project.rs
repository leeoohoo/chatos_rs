// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TaskExecutionScope {
    UserConversation {
        tenant_id: String,
        owner_user_id: String,
    },
    Project {
        tenant_id: String,
        owner_user_id: String,
        project_id: String,
    },
}

impl TaskExecutionScope {
    pub fn workspace_project_id(&self) -> Option<&str> {
        match self {
            Self::UserConversation { .. } => None,
            Self::Project { project_id, .. } => Some(project_id.as_str()),
        }
    }

    pub fn owner_user_id(&self) -> &str {
        match self {
            Self::UserConversation { owner_user_id, .. } | Self::Project { owner_user_id, .. } => {
                owner_user_id.as_str()
            }
        }
    }
}

pub fn resolve_task_execution_scope(
    project_id: Option<&str>,
    tenant_id: &str,
    owner_user_id: &str,
) -> TaskExecutionScope {
    let project_id = normalize_project_id(project_id.map(ToOwned::to_owned));
    let tenant_id = tenant_id.trim().to_string();
    let owner_user_id = owner_user_id.trim().to_string();
    match project_id {
        Some(project_id) => TaskExecutionScope::Project {
            tenant_id,
            owner_user_id,
            project_id,
        },
        None => TaskExecutionScope::UserConversation {
            tenant_id,
            owner_user_id,
        },
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum TaskProjectStatus {
    #[default]
    Active,
    Archived,
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
    pub cloud_import_source: Option<String>,
    #[serde(default)]
    pub import_status: Option<String>,
    #[serde(default)]
    pub source_git_url: Option<String>,
    #[serde(default)]
    pub harness_repo_identifier: Option<String>,
    #[serde(default)]
    pub harness_git_url: Option<String>,
    #[serde(default)]
    pub harness_default_branch: Option<String>,
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
    #[serde(default)]
    pub cloud_import_source: Option<String>,
    #[serde(default)]
    pub import_status: Option<String>,
    #[serde(default)]
    pub source_git_url: Option<String>,
    #[serde(default)]
    pub harness_repo_identifier: Option<String>,
    #[serde(default)]
    pub harness_git_url: Option<String>,
    #[serde(default)]
    pub harness_default_branch: Option<String>,
    pub description: Option<String>,
    pub status: Option<TaskProjectStatus>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
    pub archived_at: Option<String>,
}

pub fn normalize_project_id(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
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

#[cfg(test)]
mod execution_scope_tests {
    use super::*;

    #[test]
    fn user_conversation_scope_is_owner_scoped_instead_of_globally_shared() {
        let first = resolve_task_execution_scope(None, "tenant-1", "user-1");
        let second = resolve_task_execution_scope(None, "tenant-1", "user-2");

        assert_ne!(first, second);
        assert_eq!(first.workspace_project_id(), None);
        assert_eq!(first.owner_user_id(), "user-1");
    }

    #[test]
    fn concrete_project_scope_exposes_workspace_identity() {
        let scope = resolve_task_execution_scope(Some(" project-1 "), "tenant-1", "user-1");

        assert_eq!(scope.workspace_project_id(), Some("project-1"));
        assert_eq!(scope.owner_user_id(), "user-1");
    }
}
