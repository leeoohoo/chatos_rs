// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_ai_runtime::model_config::{normalize_provider, normalize_thinking_level};
use chatos_ai_runtime::ModelRuntimeConfig;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{default_tool_result_model_max_chars, default_tool_results_model_total_max_chars};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelConfigRecord {
    pub id: String,
    #[serde(default)]
    pub owner_user_id: Option<String>,
    #[serde(default)]
    pub owner_username: Option<String>,
    #[serde(default)]
    pub owner_display_name: Option<String>,
    pub name: String,
    pub provider: String,
    pub base_url: String,
    pub api_key: String,
    pub model: String,
    #[serde(default)]
    pub usage_scenario: Option<String>,
    pub temperature: Option<f64>,
    pub max_output_tokens: Option<i64>,
    pub thinking_level: Option<String>,
    pub supports_responses: bool,
    pub instructions: Option<String>,
    pub request_cwd: Option<String>,
    pub include_prompt_cache_retention: bool,
    pub request_body_limit_bytes: Option<usize>,
    pub enabled: bool,
    pub created_at: String,
    pub updated_at: String,
}

impl ModelConfigRecord {
    pub fn to_runtime_config(&self, default_request_cwd: Option<String>) -> ModelRuntimeConfig {
        let provider = runtime_provider_for_model(self.provider.as_str(), self.base_url.as_str());
        let thinking_level =
            normalize_thinking_level(provider.as_str(), self.thinking_level.as_deref())
                .ok()
                .flatten();
        ModelRuntimeConfig::openai_compatible(
            self.base_url.clone(),
            self.api_key.clone(),
            self.model.clone(),
            provider,
        )
        .with_responses_support(self.supports_responses)
        .with_instructions(self.instructions.clone())
        .with_temperature(self.temperature)
        .with_max_output_tokens(self.max_output_tokens)
        .with_thinking_level(thinking_level)
        .with_request_cwd(self.request_cwd.clone().or(default_request_cwd))
        .with_prompt_cache_retention(self.include_prompt_cache_retention)
        .with_request_body_limit_bytes(self.request_body_limit_bytes)
    }
}

fn runtime_provider_for_model(provider: &str, base_url: &str) -> String {
    let normalized = normalize_provider(provider);
    if normalized == "gpt" && !is_openai_api_base_url(base_url) {
        return "openai_compatible".to_string();
    }
    provider.to_string()
}

fn is_openai_api_base_url(base_url: &str) -> bool {
    let value = base_url.trim().to_ascii_lowercase();
    value.is_empty() || value.contains("api.openai.com")
}

#[cfg(test)]
mod tests {
    use super::ModelConfigRecord;

    #[test]
    fn execution_environment_defaults_to_cloud_on_linux_only() {
        assert_eq!(
            super::default_execution_environment_mode_for_os("linux"),
            "cloud"
        );
        assert_eq!(
            super::default_execution_environment_mode_for_os("macos"),
            "local"
        );
        assert_eq!(
            super::default_execution_environment_mode_for_os("windows"),
            "local"
        );
    }

    #[test]
    fn execution_environment_normalization_uses_platform_default_for_missing_or_invalid_values() {
        assert_eq!(
            super::normalize_execution_environment_mode_for_os(None, "linux"),
            "cloud"
        );
        assert_eq!(
            super::normalize_execution_environment_mode_for_os(Some(""), "linux"),
            "cloud"
        );
        assert_eq!(
            super::normalize_execution_environment_mode_for_os(Some("unknown"), "linux"),
            "cloud"
        );
        assert_eq!(
            super::normalize_execution_environment_mode_for_os(None, "macos"),
            "local"
        );
        assert_eq!(
            super::normalize_execution_environment_mode_for_os(Some("cloud"), "macos"),
            "cloud"
        );
    }

    #[test]
    fn runtime_config_treats_custom_openai_base_url_as_compatible() {
        let record = model_config_record("openai", "https://gateway.example.test/v1", "minimal");

        let runtime = record.to_runtime_config(None);

        assert_eq!(runtime.provider, "openai_compatible");
        assert_eq!(runtime.thinking_level.as_deref(), Some("low"));
    }

    #[test]
    fn runtime_config_keeps_openai_minimal_for_openai_base_url() {
        let record = model_config_record("openai", "https://api.openai.com/v1", "minimal");

        let runtime = record.to_runtime_config(None);

        assert_eq!(runtime.provider, "openai");
        assert_eq!(runtime.thinking_level.as_deref(), Some("minimal"));
    }

    fn model_config_record(
        provider: &str,
        base_url: &str,
        thinking_level: &str,
    ) -> ModelConfigRecord {
        ModelConfigRecord {
            id: "model-config-1".to_string(),
            owner_user_id: Some("user-1".to_string()),
            owner_username: Some("user".to_string()),
            owner_display_name: Some("User".to_string()),
            name: "Model".to_string(),
            provider: provider.to_string(),
            base_url: base_url.to_string(),
            api_key: "secret".to_string(),
            model: "model-name".to_string(),
            usage_scenario: None,
            temperature: None,
            max_output_tokens: None,
            thinking_level: Some(thinking_level.to_string()),
            supports_responses: true,
            instructions: None,
            request_cwd: None,
            include_prompt_cache_retention: false,
            request_body_limit_bytes: None,
            enabled: true,
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateModelConfigRequest {
    pub name: String,
    pub provider: String,
    pub base_url: String,
    pub api_key: String,
    pub model: String,
    pub usage_scenario: Option<String>,
    pub temperature: Option<f64>,
    pub max_output_tokens: Option<i64>,
    pub thinking_level: Option<String>,
    pub supports_responses: Option<bool>,
    pub instructions: Option<String>,
    pub request_cwd: Option<String>,
    pub include_prompt_cache_retention: Option<bool>,
    pub request_body_limit_bytes: Option<usize>,
    pub enabled: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatosSyncedModelConfigRequest {
    pub id: String,
    pub owner_user_id: Option<String>,
    pub name: String,
    pub provider: String,
    pub base_url: String,
    pub api_key: String,
    pub model: String,
    pub usage_scenario: Option<String>,
    pub temperature: Option<f64>,
    pub max_output_tokens: Option<i64>,
    pub thinking_level: Option<String>,
    pub supports_responses: Option<bool>,
    pub enabled: Option<bool>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UpdateModelConfigRequest {
    pub name: Option<String>,
    pub provider: Option<String>,
    pub base_url: Option<String>,
    pub api_key: Option<String>,
    pub model: Option<String>,
    pub usage_scenario: Option<String>,
    pub temperature: Option<f64>,
    pub max_output_tokens: Option<i64>,
    pub thinking_level: Option<String>,
    pub supports_responses: Option<bool>,
    pub instructions: Option<String>,
    pub request_cwd: Option<String>,
    pub include_prompt_cache_retention: Option<bool>,
    pub request_body_limit_bytes: Option<usize>,
    pub enabled: Option<bool>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PreviewModelCatalogRequest {
    pub provider: String,
    pub base_url: Option<String>,
    pub api_key: Option<String>,
    pub model: Option<String>,
    pub supports_responses: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderModelRecord {
    pub id: String,
    pub owned_by: Option<String>,
    pub context_length: Option<i64>,
    pub supports_images: bool,
    pub supports_video: bool,
    pub supports_reasoning: bool,
    pub supports_responses: bool,
    pub raw: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeSettingsRecord {
    pub id: String,
    pub task_execution_max_iterations: usize,
    #[serde(default)]
    pub execution_timeout_ms: Option<u64>,
    #[serde(default = "default_tool_result_model_max_chars")]
    pub tool_result_model_max_chars: usize,
    #[serde(default = "default_tool_results_model_total_max_chars")]
    pub tool_results_model_total_max_chars: usize,
    #[serde(default = "default_execution_environment_mode")]
    pub execution_environment_mode: String,
    #[serde(default)]
    pub sandbox_enabled: bool,
    #[serde(default = "default_sandbox_manager_base_url")]
    pub sandbox_manager_base_url: String,
    #[serde(default = "default_sandbox_lease_ttl_seconds")]
    pub sandbox_lease_ttl_seconds: u64,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UpdateRuntimeSettingsRequest {
    pub task_execution_max_iterations: Option<usize>,
    pub execution_timeout_ms: Option<u64>,
    pub tool_result_model_max_chars: Option<usize>,
    pub tool_results_model_total_max_chars: Option<usize>,
    pub execution_environment_mode: Option<String>,
    pub sandbox_enabled: Option<bool>,
    pub sandbox_manager_base_url: Option<String>,
    pub sandbox_lease_ttl_seconds: Option<u64>,
}

pub fn normalize_execution_environment_mode(value: Option<&str>) -> String {
    normalize_execution_environment_mode_for_os(value, std::env::consts::OS)
}

fn normalize_execution_environment_mode_for_os(value: Option<&str>, os: &str) -> String {
    let default_mode = default_execution_environment_mode_for_os(os);
    match value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(default_mode)
        .to_ascii_lowercase()
        .as_str()
    {
        "cloud" => "cloud".to_string(),
        "local" => "local".to_string(),
        _ => default_mode.to_string(),
    }
}

pub fn default_execution_environment_mode() -> String {
    default_execution_environment_mode_for_os(std::env::consts::OS).to_string()
}

fn default_execution_environment_mode_for_os(os: &str) -> &'static str {
    match os {
        "linux" => "cloud",
        _ => "local",
    }
}

pub fn default_sandbox_manager_base_url() -> String {
    "http://127.0.0.1:8095".to_string()
}

pub fn default_sandbox_lease_ttl_seconds() -> u64 {
    7_200
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelCatalogResponse {
    pub provider_config_id: Option<String>,
    pub provider: String,
    pub base_url: String,
    pub source: String,
    pub fetched_at: Option<String>,
    pub models: Vec<ProviderModelRecord>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TestModelConfigRequest {
    pub prompt: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelConfigTestResponse {
    pub ok: bool,
    pub model_config_id: String,
    pub provider: String,
    pub model: String,
    pub content: Option<String>,
    pub reasoning: Option<String>,
    pub usage: Option<Value>,
    pub response_id: Option<String>,
    pub error: Option<String>,
    pub tested_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelConfigUsageRecord {
    pub model_config_id: String,
    pub task_count: usize,
    pub run_count: usize,
}
