use chatos_ai_runtime::ModelRuntimeConfig;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{default_tool_result_model_max_chars, default_tool_results_model_total_max_chars};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelConfigRecord {
    pub id: String,
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
        ModelRuntimeConfig::openai_compatible(
            self.base_url.clone(),
            self.api_key.clone(),
            self.model.clone(),
            self.provider.clone(),
        )
        .with_responses_support(self.supports_responses)
        .with_instructions(self.instructions.clone())
        .with_temperature(self.temperature)
        .with_max_output_tokens(self.max_output_tokens)
        .with_thinking_level(self.thinking_level.clone())
        .with_request_cwd(self.request_cwd.clone().or(default_request_cwd))
        .with_prompt_cache_retention(self.include_prompt_cache_retention)
        .with_request_body_limit_bytes(self.request_body_limit_bytes)
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
    #[serde(default = "default_tool_result_model_max_chars")]
    pub tool_result_model_max_chars: usize,
    #[serde(default = "default_tool_results_model_total_max_chars")]
    pub tool_results_model_total_max_chars: usize,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UpdateRuntimeSettingsRequest {
    pub task_execution_max_iterations: Option<usize>,
    pub tool_result_model_max_chars: Option<usize>,
    pub tool_results_model_total_max_chars: Option<usize>,
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
