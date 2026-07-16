// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_ai_runtime::{
    build_responses_text_input, run_compatible_prompt_with, select_preferred_response_text,
    AiRequestHandler, SimplePromptOptions,
};
use tracing::{info, warn};
use uuid::Uuid;

use crate::models::{
    now_rfc3339, ChatosSyncedModelConfigRequest, CreateModelConfigRequest, ModelCatalogResponse,
    ModelConfigRecord, ModelConfigTestResponse, ModelConfigUsageRecord, PreviewModelCatalogRequest,
    TestModelConfigRequest, UpdateModelConfigRequest,
};
use crate::store::AppStore;

use super::model_catalog::{
    fetch_model_catalog_for_record, normalize_model_base_url_input, normalize_model_config_record,
    normalize_model_provider_input, normalize_model_thinking_level_input,
};
use super::{normalized_optional, validate_required, ModelConfigService};

mod catalog;
mod mutation;
mod testing;

impl ModelConfigService {
    pub(crate) fn new(store: AppStore) -> Self {
        Self { store }
    }

    async fn first_task_using_model_config(
        &self,
        model_config_id: &str,
    ) -> Result<Option<String>, String> {
        Ok(self
            .store
            .list_tasks()
            .await?
            .into_iter()
            .find(|task| task.default_model_config_id.as_deref() == Some(model_config_id))
            .map(|task| task.id))
    }

    async fn normalized_model_config_by_id(
        &self,
        id: &str,
    ) -> Result<Option<ModelConfigRecord>, String> {
        self.store
            .get_model_config(id)
            .await?
            .map(normalize_model_config_record)
            .transpose()
    }

    pub async fn list_model_configs(&self) -> Result<Vec<ModelConfigRecord>, String> {
        let records = self.store.list_model_configs().await?;
        records
            .into_iter()
            .map(normalize_model_config_record)
            .collect::<Result<Vec<_>, _>>()
    }

    pub async fn get_model_config(&self, id: &str) -> Result<Option<ModelConfigRecord>, String> {
        self.normalized_model_config_by_id(id).await
    }

    pub async fn upsert_chatos_model_config(
        &self,
        input: ChatosSyncedModelConfigRequest,
    ) -> Result<ModelConfigRecord, String> {
        validate_required("id", &input.id)?;
        validate_required("name", &input.name)?;
        validate_required("model", &input.model)?;
        let provider = normalize_model_provider_input(&input.provider)?;
        let thinking_level =
            normalize_model_thinking_level_input(provider.as_str(), input.thinking_level)?;
        let existing = self
            .store
            .get_model_config(input.id.trim())
            .await?
            .map(normalize_model_config_record)
            .transpose()?;
        let now = now_rfc3339();
        let record = ModelConfigRecord {
            id: input.id.trim().to_string(),
            owner_user_id: normalized_optional(input.owner_user_id),
            owner_username: existing.as_ref().and_then(|item| {
                item.owner_username
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(ToOwned::to_owned)
            }),
            owner_display_name: existing.as_ref().and_then(|item| {
                item.owner_display_name
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(ToOwned::to_owned)
            }),
            name: input.name.trim().to_string(),
            provider: provider.clone(),
            base_url: input.base_url.trim().trim_end_matches('/').to_string(),
            api_key: input.api_key.trim().to_string(),
            model: input.model.trim().to_string(),
            usage_scenario: normalized_optional(input.usage_scenario).or_else(|| {
                existing
                    .as_ref()
                    .and_then(|item| item.usage_scenario.clone())
            }),
            temperature: input
                .temperature
                .or_else(|| existing.as_ref().and_then(|item| item.temperature)),
            max_output_tokens: input
                .max_output_tokens
                .or_else(|| existing.as_ref().and_then(|item| item.max_output_tokens)),
            thinking_level,
            supports_responses: input
                .supports_responses
                .unwrap_or_else(|| provider == "openai"),
            instructions: existing.as_ref().and_then(|item| item.instructions.clone()),
            request_cwd: existing.as_ref().and_then(|item| item.request_cwd.clone()),
            include_prompt_cache_retention: existing
                .as_ref()
                .is_some_and(|item| item.include_prompt_cache_retention),
            request_body_limit_bytes: existing
                .as_ref()
                .and_then(|item| item.request_body_limit_bytes),
            enabled: input.enabled.unwrap_or(true),
            created_at: existing
                .as_ref()
                .map(|item| item.created_at.clone())
                .unwrap_or_else(|| now.clone()),
            updated_at: now,
        };
        self.store.save_model_config(record).await
    }

    pub async fn delete_model_config(&self, id: &str) -> Result<bool, String> {
        self.store.delete_model_config(id).await
    }

    pub async fn usage_stats(&self) -> Result<Vec<ModelConfigUsageRecord>, String> {
        self.store.list_model_config_usage().await
    }
}
