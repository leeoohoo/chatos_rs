// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use super::DbStatus;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum ProjectRuntimeEnvironmentStatus {
    Disabled,
    PendingConfiguration,
    #[default]
    Pending,
    Analyzing,
    Ready,
    NotRunnable,
    Failed,
}

impl DbStatus for ProjectRuntimeEnvironmentStatus {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Disabled => "disabled",
            Self::PendingConfiguration => "pending_configuration",
            Self::Pending => "pending",
            Self::Analyzing => "analyzing",
            Self::Ready => "ready",
            Self::NotRunnable => "not_runnable",
            Self::Failed => "failed",
        }
    }

    fn from_db(value: &str) -> Self {
        match value.trim() {
            "disabled" => Self::Disabled,
            "pending_configuration" => Self::PendingConfiguration,
            "analyzing" => Self::Analyzing,
            "ready" => Self::Ready,
            "not_runnable" => Self::NotRunnable,
            "failed" => Self::Failed,
            _ => Self::Pending,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum RuntimeEnvironmentProvider {
    #[default]
    None,
    LocalConnector,
    Harness,
    CloudSandboxManager,
}

impl DbStatus for RuntimeEnvironmentProvider {
    fn as_str(&self) -> &'static str {
        match self {
            Self::None => "none",
            Self::LocalConnector => "local_connector",
            Self::Harness => "harness",
            Self::CloudSandboxManager => "cloud_sandbox_manager",
        }
    }

    fn from_db(value: &str) -> Self {
        match value.trim() {
            "local_connector" => Self::LocalConnector,
            "harness" => Self::Harness,
            "cloud_sandbox_manager" => Self::CloudSandboxManager,
            _ => Self::None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectRuntimeEnvironmentRecord {
    pub project_id: String,
    pub status: ProjectRuntimeEnvironmentStatus,
    pub sandbox_enabled: bool,
    pub sandbox_provider: RuntimeEnvironmentProvider,
    pub file_provider: RuntimeEnvironmentProvider,
    pub analysis_summary: Option<String>,
    pub not_runnable_reason: Option<String>,
    #[serde(default = "empty_object")]
    pub detected_stack: Value,
    #[serde(default = "empty_array")]
    pub required_services: Value,
    #[serde(default = "empty_object")]
    pub env_vars: Value,
    pub last_agent_run_id: Option<String>,
    pub last_error: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectRuntimeEnvironmentImageRecord {
    pub id: String,
    pub project_id: String,
    pub environment_key: String,
    pub environment_type: String,
    pub display_name: String,
    pub image_id: Option<String>,
    pub image_ref: Option<String>,
    pub image_provider: RuntimeEnvironmentProvider,
    #[serde(default = "empty_array")]
    pub features: Value,
    #[serde(default = "empty_array")]
    pub ports: Value,
    #[serde(default = "empty_object")]
    pub env_vars: Value,
    pub status: String,
    pub error: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectRuntimeEnvironmentResponse {
    pub environment: ProjectRuntimeEnvironmentRecord,
    #[serde(default)]
    pub images: Vec<ProjectRuntimeEnvironmentImageRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectRuntimeEnvironmentProgressResponse {
    pub project_id: String,
    pub run_id: Option<String>,
    pub phase: String,
    pub status: String,
    pub progress_percent: Option<u8>,
    pub provider: RuntimeEnvironmentProvider,
    pub job_id: Option<String>,
    pub image_id: Option<String>,
    pub image_ref: Option<String>,
    pub started_at: Option<String>,
    pub updated_at: String,
    pub finished_at: Option<String>,
    pub logs: String,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UpdateProjectRuntimeEnvironmentSettingsRequest {
    pub sandbox_enabled: Option<bool>,
}

pub fn empty_object() -> Value {
    json!({})
}

pub fn empty_array() -> Value {
    json!([])
}
