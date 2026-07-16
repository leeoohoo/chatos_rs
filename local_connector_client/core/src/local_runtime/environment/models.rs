// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::FromRow;

#[derive(Debug, Clone, Serialize, FromRow)]
pub(crate) struct LocalRuntimeEnvironmentRecord {
    pub(crate) project_id: String,
    pub(crate) owner_user_id: String,
    pub(crate) status: String,
    pub(crate) sandbox_enabled: bool,
    pub(crate) sandbox_provider: String,
    pub(crate) file_provider: String,
    pub(crate) analysis_summary: Option<String>,
    pub(crate) not_runnable_reason: Option<String>,
    pub(crate) detected_stack_json: String,
    pub(crate) required_services_json: String,
    pub(crate) env_vars_json: String,
    pub(crate) generated_config_files_json: String,
    pub(crate) last_agent_run_id: Option<String>,
    pub(crate) last_error: Option<String>,
    pub(crate) created_at: String,
    pub(crate) updated_at: String,
}

#[derive(Debug, Clone, Serialize, FromRow)]
pub(crate) struct LocalRuntimeEnvironmentImageRecord {
    pub(crate) id: String,
    pub(crate) project_id: String,
    pub(crate) environment_key: String,
    pub(crate) environment_type: String,
    pub(crate) display_name: String,
    pub(crate) image_id: Option<String>,
    pub(crate) image_ref: Option<String>,
    pub(crate) image_provider: String,
    pub(crate) dockerfile: Option<String>,
    pub(crate) features_json: String,
    pub(crate) ports_json: String,
    pub(crate) env_vars_json: String,
    pub(crate) status: String,
    pub(crate) error: Option<String>,
    pub(crate) created_at: String,
    pub(crate) updated_at: String,
}

#[derive(Debug, Clone, Serialize, FromRow)]
pub(crate) struct LocalEnvironmentProgressRecord {
    pub(crate) project_id: String,
    pub(crate) run_id: Option<String>,
    pub(crate) phase: String,
    pub(crate) status: String,
    pub(crate) progress_percent: Option<i64>,
    pub(crate) provider: String,
    pub(crate) started_at: Option<String>,
    pub(crate) updated_at: String,
    pub(crate) finished_at: Option<String>,
    pub(crate) logs: String,
    pub(crate) error: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub(crate) struct LocalEnvironmentAnalysisResult {
    pub(crate) status: String,
    pub(crate) analysis_summary: String,
    pub(crate) not_runnable_reason: Option<String>,
    #[serde(default)]
    pub(crate) detected_stack: Value,
    #[serde(default)]
    pub(crate) required_services: Value,
    #[serde(default, alias = "environment_variables")]
    pub(crate) env_vars: Value,
    #[serde(default)]
    pub(crate) generated_config_files: Value,
    #[serde(default)]
    pub(crate) images: Vec<LocalEnvironmentImagePlan>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub(crate) struct LocalEnvironmentImagePlan {
    pub(crate) environment_key: String,
    #[serde(default = "default_environment_type")]
    pub(crate) environment_type: String,
    pub(crate) display_name: String,
    pub(crate) image_ref: Option<String>,
    #[serde(default)]
    pub(crate) dockerfile: Option<String>,
    #[serde(default)]
    pub(crate) features: Value,
    #[serde(default)]
    pub(crate) ports: Value,
    #[serde(default)]
    pub(crate) env_vars: Value,
}

fn default_environment_type() -> String {
    "application".to_string()
}
