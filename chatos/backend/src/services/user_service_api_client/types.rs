// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
pub struct UserServiceAuthUser {
    pub id: String,
    pub username: Option<String>,
    pub display_name: Option<String>,
    pub role: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UserServiceLoginResponse {
    pub token: String,
    pub user: UserServiceAuthUser,
}

#[derive(Debug, Deserialize)]
pub struct UserServiceLocalConnectorTicketResponse {
    pub ticket: String,
    pub expires_in_seconds: i64,
}

#[derive(Debug, Deserialize)]
pub struct UserServiceMeResponse {
    pub user: UserServiceAuthUser,
}

#[derive(Debug, Deserialize)]
pub struct UserServiceVerifiedPrincipal {
    pub principal_type: String,
    pub user_id: Option<String>,
    pub role: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UserServiceVerifyResponse {
    pub principal: UserServiceVerifiedPrincipal,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct UserServiceAgentAccountSummary {
    pub id: String,
    pub username: String,
    pub display_name: String,
    pub owner_user_id: String,
    pub owner_username: String,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct CreateUserServiceAgentAccountRequest {
    pub username: String,
    pub display_name: Option<String>,
    pub password: String,
    pub owner_user_id: Option<String>,
    pub enabled: Option<bool>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct UserServiceModelConfigRecord {
    pub id: String,
    pub owner_user_id: String,
    pub name: String,
    pub provider: String,
    #[serde(default)]
    pub prompt_vendor: Option<String>,
    #[serde(default)]
    pub model: String,
    #[serde(default)]
    pub model_name: String,
    pub thinking_level: Option<String>,
    pub task_usage_scenario: Option<String>,
    pub task_thinking_level: Option<String>,
    pub temperature: Option<f64>,
    pub max_output_tokens: Option<i64>,
    pub api_key: Option<String>,
    #[serde(default)]
    pub has_api_key: bool,
    pub base_url: Option<String>,
    pub enabled: bool,
    #[serde(default)]
    pub supports_images: bool,
    #[serde(default)]
    pub supports_reasoning: bool,
    #[serde(default)]
    pub supports_responses: bool,
    pub created_at: String,
    pub updated_at: String,
    #[serde(default)]
    pub sync_warnings: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct UserServiceModelProviderRecord {
    pub id: String,
    pub owner_user_id: String,
    pub name: String,
    pub provider: String,
    #[serde(default)]
    pub prompt_vendor: Option<String>,
    pub api_key: Option<String>,
    #[serde(default)]
    pub has_api_key: bool,
    pub base_url: Option<String>,
    pub enabled: bool,
    #[serde(default)]
    pub supports_images: bool,
    #[serde(default)]
    pub supports_reasoning: bool,
    #[serde(default)]
    pub supports_responses: bool,
    pub last_sync_status: Option<String>,
    pub last_sync_error: Option<String>,
    pub last_synced_at: Option<String>,
    #[serde(default)]
    pub imported_model_count: i64,
    pub created_at: String,
    pub updated_at: String,
    #[serde(default)]
    pub sync_warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CreateUserServiceModelConfigRequest {
    pub id: Option<String>,
    pub owner_user_id: Option<String>,
    pub name: String,
    pub provider: Option<String>,
    pub prompt_vendor: Option<String>,
    pub model: Option<String>,
    pub thinking_level: Option<String>,
    pub task_usage_scenario: Option<String>,
    pub task_thinking_level: Option<String>,
    pub temperature: Option<f64>,
    pub max_output_tokens: Option<i64>,
    pub api_key: Option<String>,
    pub base_url: Option<String>,
    pub enabled: Option<bool>,
    pub supports_images: Option<bool>,
    pub supports_reasoning: Option<bool>,
    pub supports_responses: Option<bool>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CreateUserServiceModelProviderRequest {
    pub id: Option<String>,
    pub owner_user_id: Option<String>,
    pub name: String,
    pub provider: Option<String>,
    pub prompt_vendor: Option<String>,
    pub api_key: Option<String>,
    pub base_url: Option<String>,
    pub enabled: Option<bool>,
    pub supports_images: Option<bool>,
    pub supports_reasoning: Option<bool>,
    pub supports_responses: Option<bool>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct UpdateUserServiceModelConfigRequest {
    pub name: Option<String>,
    pub provider: Option<String>,
    pub prompt_vendor: Option<String>,
    pub model: Option<String>,
    pub thinking_level: Option<String>,
    pub task_usage_scenario: Option<String>,
    pub task_thinking_level: Option<String>,
    pub temperature: Option<f64>,
    pub clear_temperature: Option<bool>,
    pub max_output_tokens: Option<i64>,
    pub clear_max_output_tokens: Option<bool>,
    pub api_key: Option<String>,
    pub clear_api_key: Option<bool>,
    pub base_url: Option<String>,
    pub enabled: Option<bool>,
    pub supports_images: Option<bool>,
    pub supports_reasoning: Option<bool>,
    pub supports_responses: Option<bool>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct UpdateUserServiceModelProviderRequest {
    pub name: Option<String>,
    pub provider: Option<String>,
    pub prompt_vendor: Option<String>,
    pub api_key: Option<String>,
    pub clear_api_key: Option<bool>,
    pub base_url: Option<String>,
    pub enabled: Option<bool>,
    pub supports_images: Option<bool>,
    pub supports_reasoning: Option<bool>,
    pub supports_responses: Option<bool>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct UserServiceModelSettingsRecord {
    pub user_id: String,
    pub memory_summary_model_config_id: Option<String>,
    pub memory_summary_thinking_level: Option<String>,
    pub project_management_agent_model_config_id: Option<String>,
    pub project_management_agent_thinking_level: Option<String>,
    pub environment_initialization_model_config_id: Option<String>,
    pub environment_initialization_thinking_level: Option<String>,
    pub updated_at: String,
    #[serde(default)]
    pub sync_warnings: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct UpdateUserServiceModelSettingsRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memory_summary_model_config_id: Option<Option<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memory_summary_thinking_level: Option<Option<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_management_agent_model_config_id: Option<Option<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_management_agent_thinking_level: Option<Option<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub environment_initialization_model_config_id: Option<Option<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub environment_initialization_thinking_level: Option<Option<String>>,
}
