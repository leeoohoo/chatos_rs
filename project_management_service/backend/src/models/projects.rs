// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde::{Deserialize, Serialize};

use super::DbStatus;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum ProjectStatus {
    #[default]
    Active,
    Archived,
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

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum ProjectSourceType {
    Local,
    LocalConnector,
    #[default]
    Cloud,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum ProjectExecutionPlane {
    LocalConnector,
    #[default]
    Cloud,
}

impl DbStatus for ProjectExecutionPlane {
    fn as_str(&self) -> &'static str {
        match self {
            Self::LocalConnector => "local_connector",
            Self::Cloud => "cloud",
        }
    }

    fn from_db(value: &str) -> Self {
        match value.trim() {
            "local_connector" => Self::LocalConnector,
            _ => Self::Cloud,
        }
    }
}

impl ProjectSourceType {
    pub fn execution_plane(self) -> ProjectExecutionPlane {
        match self {
            Self::Cloud => ProjectExecutionPlane::Cloud,
            Self::Local | Self::LocalConnector => ProjectExecutionPlane::LocalConnector,
        }
    }
}

impl DbStatus for ProjectSourceType {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Local => "local",
            Self::LocalConnector => "local_connector",
            Self::Cloud => "cloud",
        }
    }

    fn from_db(value: &str) -> Self {
        match value.trim() {
            "local" => Self::Local,
            "local_connector" => Self::LocalConnector,
            _ => Self::Cloud,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum CloudImportSource {
    #[default]
    None,
    Empty,
    Git,
    Zip,
}

impl DbStatus for CloudImportSource {
    fn as_str(&self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Empty => "empty",
            Self::Git => "git",
            Self::Zip => "zip",
        }
    }

    fn from_db(value: &str) -> Self {
        match value.trim() {
            "empty" => Self::Empty,
            "git" => Self::Git,
            "zip" => Self::Zip,
            _ => Self::None,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum ProjectImportStatus {
    #[default]
    None,
    Pending,
    Importing,
    Ready,
    Failed,
}

impl DbStatus for ProjectImportStatus {
    fn as_str(&self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Pending => "pending",
            Self::Importing => "importing",
            Self::Ready => "ready",
            Self::Failed => "failed",
        }
    }

    fn from_db(value: &str) -> Self {
        match value.trim() {
            "pending" => Self::Pending,
            "importing" => Self::Importing,
            "ready" => Self::Ready,
            "failed" => Self::Failed,
            _ => Self::None,
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
    #[serde(default)]
    pub source_type: ProjectSourceType,
    #[serde(default)]
    pub execution_plane: ProjectExecutionPlane,
    #[serde(default)]
    pub cloud_import_source: CloudImportSource,
    #[serde(default)]
    pub import_status: ProjectImportStatus,
    #[serde(default)]
    pub source_git_url: Option<String>,
    #[serde(default)]
    pub harness_space_identifier: Option<String>,
    #[serde(default)]
    pub harness_repo_identifier: Option<String>,
    #[serde(default)]
    pub harness_repo_path: Option<String>,
    #[serde(default)]
    pub harness_git_url: Option<String>,
    #[serde(default)]
    pub harness_git_ssh_url: Option<String>,
    #[serde(default)]
    pub harness_default_branch: Option<String>,
    #[serde(default)]
    pub harness_provision_status: Option<String>,
    #[serde(default)]
    pub harness_provision_error: Option<String>,
    #[serde(default)]
    pub harness_provisioned_at: Option<String>,
    #[serde(default)]
    pub import_error: Option<String>,
    #[serde(default)]
    pub import_started_at: Option<String>,
    #[serde(default)]
    pub import_finished_at: Option<String>,
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
    #[serde(default)]
    pub sandbox_enabled: Option<bool>,
    #[serde(default)]
    pub source_type: Option<ProjectSourceType>,
    #[serde(default)]
    pub cloud_import_source: Option<CloudImportSource>,
    #[serde(default)]
    pub import_status: Option<ProjectImportStatus>,
    #[serde(default)]
    pub source_git_url: Option<String>,
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
    #[serde(default)]
    pub sandbox_enabled: Option<bool>,
    #[serde(default)]
    pub source_type: Option<ProjectSourceType>,
    #[serde(default)]
    pub cloud_import_source: Option<CloudImportSource>,
    #[serde(default)]
    pub import_status: Option<ProjectImportStatus>,
    #[serde(default)]
    pub source_git_url: Option<String>,
    #[serde(default)]
    pub harness_space_identifier: Option<String>,
    #[serde(default)]
    pub harness_repo_identifier: Option<String>,
    #[serde(default)]
    pub harness_repo_path: Option<String>,
    #[serde(default)]
    pub harness_git_url: Option<String>,
    #[serde(default)]
    pub harness_git_ssh_url: Option<String>,
    #[serde(default)]
    pub harness_default_branch: Option<String>,
    #[serde(default)]
    pub harness_provision_status: Option<String>,
    #[serde(default)]
    pub harness_provision_error: Option<String>,
    #[serde(default)]
    pub harness_provisioned_at: Option<String>,
    #[serde(default)]
    pub import_error: Option<String>,
    #[serde(default)]
    pub import_started_at: Option<String>,
    #[serde(default)]
    pub import_finished_at: Option<String>,
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
