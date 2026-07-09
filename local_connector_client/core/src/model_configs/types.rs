// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub(crate) struct ModelConfigState {
    #[serde(default)]
    pub(crate) configs: Vec<LocalModelConfigRecord>,
    #[serde(default)]
    pub(crate) settings: LocalModelSettings,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct LocalModelConfigRecord {
    pub(crate) id: String,
    #[serde(default)]
    pub(crate) server_model_config_id: Option<String>,
    pub(crate) name: String,
    pub(crate) provider: String,
    pub(crate) model: String,
    #[serde(default)]
    pub(crate) base_url: Option<String>,
    #[serde(default)]
    pub(crate) api_key: Option<String>,
    #[serde(default)]
    pub(crate) enabled: bool,
    #[serde(default)]
    pub(crate) supports_images: bool,
    #[serde(default)]
    pub(crate) supports_reasoning: bool,
    #[serde(default)]
    pub(crate) supports_responses: bool,
    #[serde(default)]
    pub(crate) thinking_level: Option<String>,
    #[serde(default)]
    pub(crate) task_usage_scenario: Option<String>,
    #[serde(default)]
    pub(crate) task_thinking_level: Option<String>,
    #[serde(default)]
    pub(crate) temperature: Option<f64>,
    #[serde(default)]
    pub(crate) max_output_tokens: Option<i64>,
    pub(crate) created_at: String,
    pub(crate) updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct LocalModelConfigDraft {
    pub(crate) id: Option<String>,
    pub(crate) server_model_config_id: Option<String>,
    pub(crate) name: String,
    pub(crate) provider: Option<String>,
    pub(crate) model: Option<String>,
    pub(crate) base_url: Option<String>,
    pub(crate) api_key: Option<String>,
    #[serde(default)]
    pub(crate) copy_api_key_from_id: Option<String>,
    pub(crate) clear_api_key: Option<bool>,
    pub(crate) enabled: Option<bool>,
    pub(crate) supports_images: Option<bool>,
    pub(crate) supports_reasoning: Option<bool>,
    pub(crate) supports_responses: Option<bool>,
    pub(crate) thinking_level: Option<String>,
    pub(crate) task_usage_scenario: Option<String>,
    pub(crate) task_thinking_level: Option<String>,
    pub(crate) temperature: Option<f64>,
    #[serde(default)]
    pub(crate) clear_temperature: Option<bool>,
    pub(crate) max_output_tokens: Option<i64>,
    #[serde(default)]
    pub(crate) clear_max_output_tokens: Option<bool>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct LocalModelConfigPublic {
    pub(crate) id: String,
    pub(crate) server_model_config_id: Option<String>,
    pub(crate) name: String,
    pub(crate) provider: String,
    pub(crate) model: String,
    pub(crate) model_name: String,
    pub(crate) base_url: Option<String>,
    pub(crate) has_api_key: bool,
    pub(crate) enabled: bool,
    pub(crate) supports_images: bool,
    pub(crate) supports_reasoning: bool,
    pub(crate) supports_responses: bool,
    pub(crate) thinking_level: Option<String>,
    pub(crate) task_usage_scenario: Option<String>,
    pub(crate) task_thinking_level: Option<String>,
    pub(crate) temperature: Option<f64>,
    pub(crate) max_output_tokens: Option<i64>,
    pub(crate) created_at: String,
    pub(crate) updated_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct LocalProviderModelRecord {
    pub(crate) id: String,
    pub(crate) owned_by: Option<String>,
    pub(crate) context_length: Option<i64>,
    pub(crate) supports_images: bool,
    pub(crate) supports_reasoning: bool,
    pub(crate) supports_responses: bool,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct LocalModelCatalogResponse {
    pub(crate) provider: String,
    pub(crate) base_url: String,
    pub(crate) source: String,
    pub(crate) fetched_at: Option<String>,
    pub(crate) models: Vec<LocalProviderModelRecord>,
    pub(crate) error: Option<String>,
}

impl LocalModelConfigRecord {
    pub(crate) fn public_value(&self) -> LocalModelConfigPublic {
        LocalModelConfigPublic {
            id: self.id.clone(),
            server_model_config_id: self.server_model_config_id.clone(),
            name: self.name.clone(),
            provider: self.provider.clone(),
            model: self.model.clone(),
            model_name: self.model.clone(),
            base_url: self.base_url.clone(),
            has_api_key: self
                .api_key
                .as_deref()
                .map(str::trim)
                .is_some_and(|value| !value.is_empty()),
            enabled: self.enabled,
            supports_images: self.supports_images,
            supports_reasoning: self.supports_reasoning,
            supports_responses: self.supports_responses,
            thinking_level: self.thinking_level.clone(),
            task_usage_scenario: self.task_usage_scenario.clone(),
            task_thinking_level: self.task_thinking_level.clone(),
            temperature: self.temperature,
            max_output_tokens: self.max_output_tokens,
            created_at: self.created_at.clone(),
            updated_at: self.updated_at.clone(),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub(crate) struct LocalModelSettings {
    #[serde(default)]
    pub(crate) memory_summary_model_config_id: Option<String>,
    #[serde(default)]
    pub(crate) memory_summary_thinking_level: Option<String>,
    #[serde(default)]
    pub(crate) project_management_agent_model_config_id: Option<String>,
    #[serde(default)]
    pub(crate) project_management_agent_thinking_level: Option<String>,
    #[serde(default)]
    pub(crate) command_approval_model_config_id: Option<String>,
    #[serde(default)]
    pub(crate) command_approval_thinking_level: Option<String>,
    #[serde(default)]
    pub(crate) updated_at: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct LocalModelRuntimeResponse {
    pub(crate) id: String,
    pub(crate) local_model_config_id: String,
    pub(crate) provider: String,
    pub(crate) base_url: String,
    pub(crate) api_key: String,
    pub(crate) model: String,
    pub(crate) thinking_level: Option<String>,
    pub(crate) supports_images: bool,
    pub(crate) supports_reasoning: bool,
    pub(crate) supports_responses: bool,
    pub(crate) temperature: Option<f64>,
    pub(crate) max_output_tokens: Option<i64>,
}
