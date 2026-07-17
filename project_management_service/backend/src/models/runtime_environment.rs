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
    PendingImageBuild,
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
            Self::PendingImageBuild => "pending_image_build",
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
            "pending_image_build" => Self::PendingImageBuild,
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

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum RuntimeEnvironmentVariableSource {
    Project,
    AiRecommended,
    User,
    #[default]
    None,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeServiceRole {
    Application,
    Dependency,
    #[default]
    Unknown,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeMcpPolicyManager {
    #[default]
    System,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeMcpAttachment {
    ProjectGatewayTarget,
    #[default]
    None,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProgramManagedMcpPolicy {
    #[serde(default)]
    pub managed_by: RuntimeMcpPolicyManager,
    #[serde(default)]
    pub attachment: RuntimeMcpAttachment,
    #[serde(default)]
    pub filesystem: bool,
    #[serde(default)]
    pub terminal: bool,
}

impl Default for ProgramManagedMcpPolicy {
    fn default() -> Self {
        Self {
            managed_by: RuntimeMcpPolicyManager::System,
            attachment: RuntimeMcpAttachment::None,
            filesystem: false,
            terminal: false,
        }
    }
}

impl ProgramManagedMcpPolicy {
    pub fn application_target() -> Self {
        Self {
            managed_by: RuntimeMcpPolicyManager::System,
            attachment: RuntimeMcpAttachment::ProjectGatewayTarget,
            filesystem: true,
            terminal: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectRuntimeEnvironmentVariableRecord {
    pub name: String,
    #[serde(default)]
    pub project_value: Option<String>,
    #[serde(default = "default_true")]
    pub project_value_suitable: bool,
    #[serde(default)]
    pub recommended_value: Option<String>,
    #[serde(default)]
    pub user_value: Option<String>,
    #[serde(default)]
    pub effective_value: Option<String>,
    #[serde(default)]
    pub effective_source: RuntimeEnvironmentVariableSource,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub recommendation_reason: Option<String>,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub secret: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectRuntimeEnvironmentConfigFileRecord {
    pub path: String,
    pub format: String,
    pub content: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub source_files: Vec<String>,
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
    #[serde(default)]
    pub environment_variables: Vec<ProjectRuntimeEnvironmentVariableRecord>,
    #[serde(default)]
    pub generated_config_files: Vec<ProjectRuntimeEnvironmentConfigFileRecord>,
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
    #[serde(default)]
    pub service_id: String,
    #[serde(default)]
    pub service_role: RuntimeServiceRole,
    #[serde(default)]
    pub mcp_policy: ProgramManagedMcpPolicy,
    pub image_id: Option<String>,
    pub image_ref: Option<String>,
    pub image_provider: RuntimeEnvironmentProvider,
    #[serde(default = "empty_array")]
    pub features: Value,
    #[serde(default = "empty_array")]
    pub ports: Value,
    #[serde(default = "empty_object")]
    pub env_vars: Value,
    #[serde(default)]
    pub dockerfile: Option<String>,
    #[serde(default)]
    pub custom_build_script: Option<String>,
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

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UpdateProjectRuntimeEnvironmentVariablesRequest {
    #[serde(default)]
    pub variables: Vec<ProjectRuntimeEnvironmentVariableOverride>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectRuntimeEnvironmentVariableOverride {
    pub name: String,
    pub value: String,
}

pub fn empty_object() -> Value {
    json!({})
}

pub fn empty_array() -> Value {
    json!([])
}

fn default_true() -> bool {
    true
}
