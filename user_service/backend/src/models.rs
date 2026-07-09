// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde::{Deserialize, Serialize};

pub const USER_ROLE_SUPER_ADMIN: &str = "super_admin";
pub const USER_ROLE_USER: &str = "user";
pub const PRINCIPAL_TYPE_HUMAN_USER: &str = "human_user";
pub const PRINCIPAL_TYPE_AGENT_ACCOUNT: &str = "agent_account";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserRecord {
    pub id: String,
    pub username: String,
    pub display_name: String,
    pub password_hash: String,
    pub role: String,
    pub enabled: bool,
    pub created_at: String,
    pub updated_at: String,
    pub last_login_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserSummaryRecord {
    pub id: String,
    pub username: String,
    pub display_name: String,
    pub role: String,
    pub enabled: bool,
    pub created_at: String,
    pub updated_at: String,
    pub last_login_at: Option<String>,
    pub agent_count: i64,
    pub harness_provisioning: Option<HarnessProvisioningSummaryRecord>,
}

pub const HARNESS_PROVISIONING_STATUS_PENDING: &str = "pending";
pub const HARNESS_PROVISIONING_STATUS_PROVISIONED: &str = "provisioned";
pub const HARNESS_PROVISIONING_STATUS_FAILED: &str = "failed";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HarnessProvisioningRecord {
    pub user_id: String,
    pub username: String,
    pub harness_uid: String,
    pub harness_email: String,
    pub space_identifier: String,
    pub status: String,
    pub attempts: i64,
    pub encrypted_password: Option<String>,
    #[serde(default)]
    pub encrypted_access_token: Option<String>,
    #[serde(default)]
    pub access_token_identifier: Option<String>,
    #[serde(default)]
    pub access_token_created_at: Option<String>,
    pub last_error: Option<String>,
    pub last_attempt_at: Option<String>,
    pub provisioned_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HarnessProvisioningSummaryRecord {
    pub status: String,
    pub harness_uid: String,
    pub harness_email: String,
    pub space_identifier: String,
    pub attempts: i64,
    pub last_error: Option<String>,
    pub last_attempt_at: Option<String>,
    pub provisioned_at: Option<String>,
    pub updated_at: String,
}

impl From<HarnessProvisioningRecord> for HarnessProvisioningSummaryRecord {
    fn from(value: HarnessProvisioningRecord) -> Self {
        Self {
            status: value.status,
            harness_uid: value.harness_uid,
            harness_email: value.harness_email,
            space_identifier: value.space_identifier,
            attempts: value.attempts,
            last_error: value.last_error,
            last_attempt_at: value.last_attempt_at,
            provisioned_at: value.provisioned_at,
            updated_at: value.updated_at,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentAccountRecord {
    pub id: String,
    pub username: String,
    pub display_name: String,
    pub password_hash: String,
    pub owner_user_id: String,
    pub enabled: bool,
    pub created_at: String,
    pub updated_at: String,
    pub last_login_at: Option<String>,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserModelConfigRecord {
    pub id: String,
    pub owner_user_id: String,
    pub name: String,
    pub provider: String,
    pub model: String,
    pub thinking_level: Option<String>,
    pub task_usage_scenario: Option<String>,
    pub task_thinking_level: Option<String>,
    pub api_key: Option<String>,
    #[serde(default)]
    pub has_api_key: bool,
    pub base_url: Option<String>,
    pub enabled: bool,
    pub supports_images: bool,
    pub supports_reasoning: bool,
    pub supports_responses: bool,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserModelProviderRecord {
    pub id: String,
    pub owner_user_id: String,
    pub name: String,
    pub provider: String,
    pub api_key: Option<String>,
    #[serde(default)]
    pub has_api_key: bool,
    pub base_url: Option<String>,
    pub enabled: bool,
    pub supports_images: bool,
    pub supports_reasoning: bool,
    pub supports_responses: bool,
    pub last_sync_status: Option<String>,
    pub last_sync_error: Option<String>,
    pub last_synced_at: Option<String>,
    #[serde(default)]
    pub imported_model_count: i64,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserModelSettingsRecord {
    pub user_id: String,
    #[serde(default)]
    pub memory_summary_model_config_id: Option<String>,
    #[serde(default)]
    pub memory_summary_thinking_level: Option<String>,
    #[serde(default)]
    pub project_management_agent_model_config_id: Option<String>,
    #[serde(default)]
    pub project_management_agent_thinking_level: Option<String>,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthUser {
    pub id: String,
    pub username: String,
    pub display_name: String,
    pub role: String,
    pub principal_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegisterRequest {
    pub username: String,
    pub display_name: Option<String>,
    pub password: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoginResponse {
    pub token: String,
    pub user: AuthUser,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CurrentUserResponse {
    pub user: AuthUser,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerifiedPrincipal {
    pub sub: String,
    pub jti: String,
    pub exp: usize,
    pub principal_type: String,
    pub user_id: Option<String>,
    pub username: Option<String>,
    pub display_name: Option<String>,
    pub role: Option<String>,
    pub agent_account_id: Option<String>,
    pub owner_user_id: Option<String>,
    pub owner_username: Option<String>,
    pub owner_display_name: Option<String>,
    #[serde(default)]
    pub scopes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenVerifyResponse {
    pub principal: VerifiedPrincipal,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateUserRequest {
    pub username: String,
    pub display_name: Option<String>,
    pub password: String,
    pub role: Option<String>,
    pub enabled: Option<bool>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UpdateUserRequest {
    pub display_name: Option<String>,
    pub password: Option<String>,
    pub role: Option<String>,
    pub enabled: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProvisionHarnessUserRequest {
    pub password: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateAgentAccountRequest {
    pub username: String,
    pub display_name: Option<String>,
    pub password: String,
    pub owner_user_id: Option<String>,
    pub enabled: Option<bool>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UpdateAgentAccountRequest {
    pub display_name: Option<String>,
    pub owner_user_id: Option<String>,
    pub enabled: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResetAgentPasswordRequest {
    pub password: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateUserModelConfigRequest {
    pub id: Option<String>,
    pub owner_user_id: Option<String>,
    pub name: String,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub thinking_level: Option<String>,
    pub task_usage_scenario: Option<String>,
    pub task_thinking_level: Option<String>,
    pub api_key: Option<String>,
    #[serde(default)]
    pub has_api_key: Option<bool>,
    pub base_url: Option<String>,
    pub enabled: Option<bool>,
    pub supports_images: Option<bool>,
    pub supports_reasoning: Option<bool>,
    pub supports_responses: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateUserModelProviderRequest {
    pub id: Option<String>,
    pub owner_user_id: Option<String>,
    pub name: String,
    pub provider: Option<String>,
    pub api_key: Option<String>,
    #[serde(default)]
    pub has_api_key: Option<bool>,
    pub base_url: Option<String>,
    pub enabled: Option<bool>,
    pub supports_images: Option<bool>,
    pub supports_reasoning: Option<bool>,
    pub supports_responses: Option<bool>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UpdateUserModelConfigRequest {
    pub name: Option<String>,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub thinking_level: Option<String>,
    pub task_usage_scenario: Option<String>,
    pub task_thinking_level: Option<String>,
    pub api_key: Option<String>,
    pub has_api_key: Option<bool>,
    pub clear_api_key: Option<bool>,
    pub base_url: Option<String>,
    pub enabled: Option<bool>,
    pub supports_images: Option<bool>,
    pub supports_reasoning: Option<bool>,
    pub supports_responses: Option<bool>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UpdateUserModelProviderRequest {
    pub name: Option<String>,
    pub provider: Option<String>,
    pub api_key: Option<String>,
    pub has_api_key: Option<bool>,
    pub clear_api_key: Option<bool>,
    pub base_url: Option<String>,
    pub enabled: Option<bool>,
    pub supports_images: Option<bool>,
    pub supports_reasoning: Option<bool>,
    pub supports_responses: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateUserModelSettingsRequest {
    pub user_id: Option<String>,
    pub memory_summary_model_config_id: Option<Option<String>>,
    pub memory_summary_thinking_level: Option<Option<String>>,
    pub project_management_agent_model_config_id: Option<Option<String>>,
    pub project_management_agent_thinking_level: Option<Option<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskRunnerTokenExchangeRequest {
    #[serde(alias = "agent_account_id")]
    pub task_runner_agent_account_id: String,
    pub contact_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenExchangePrincipalSummary {
    pub principal_type: String,
    pub agent_account_id: String,
    pub owner_user_id: String,
    pub owner_username: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskRunnerTokenExchangeResponse {
    pub access_token: String,
    pub token_type: String,
    pub expires_in: i64,
    pub principal: TokenExchangePrincipalSummary,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthResponse {
    pub status: String,
    pub service: String,
    pub now: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemConfigResponse {
    pub service: String,
    pub issuer: String,
    pub user_service_audience: String,
    pub task_runner_audience: String,
    pub database_url: String,
    pub user_access_ttl_seconds: i64,
    pub task_runner_access_ttl_seconds: i64,
}
