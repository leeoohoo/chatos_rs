use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectRunToolchainOption {
    pub id: String,
    pub kind: String,
    pub label: String,
    pub version: Option<String>,
    pub path: String,
    pub source: String,
    pub is_default: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProjectRunConfigFileSummary {
    pub kind: String,
    pub label: String,
    pub path: String,
    pub preview: Option<String>,
    pub source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProjectRunValidationIssue {
    pub kind: String,
    pub message: String,
    pub target_id: Option<String>,
    pub target_label: Option<String>,
    pub path: Option<String>,
    pub hint: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProjectRunCustomToolchain {
    pub kind: String,
    pub label: String,
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProjectRunEnvironmentSelection {
    pub project_id: String,
    pub user_id: Option<String>,
    pub selected_toolchains: HashMap<String, String>,
    pub custom_toolchains: HashMap<String, ProjectRunCustomToolchain>,
    pub env_vars: HashMap<String, String>,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProjectRunEnvironmentSnapshot {
    pub project_id: String,
    pub user_id: Option<String>,
    pub options_by_kind: HashMap<String, Vec<ProjectRunToolchainOption>>,
    pub config_files: Vec<ProjectRunConfigFileSummary>,
    pub validation_issues: Vec<ProjectRunValidationIssue>,
    pub selected_toolchains: HashMap<String, String>,
    pub custom_toolchains: HashMap<String, ProjectRunCustomToolchain>,
    pub env_vars: HashMap<String, String>,
    pub updated_at: Option<String>,
}
