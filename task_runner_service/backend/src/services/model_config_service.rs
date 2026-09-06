// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use tracing::warn;

use crate::models::ModelConfigRecord;
#[cfg(test)]
use crate::models::{now_rfc3339, ChatosSyncedModelConfigRequest};
use crate::store::AppStore;

use super::model_catalog::normalize_model_config_record;
#[cfg(test)]
use super::model_catalog::{
    normalize_model_prompt_vendor_input, normalize_model_provider_input,
    normalize_model_thinking_level_input,
};
use super::ModelConfigService;
#[cfg(test)]
use super::{normalized_optional, validate_required};

impl ModelConfigService {
    pub(crate) fn new(store: AppStore) -> Self {
        Self { store }
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
        Ok(records
            .into_iter()
            .filter_map(|record| {
                let record_id = record.id.clone();
                match normalize_model_config_record(record) {
                    Ok(record) => Some(record),
                    Err(err) => {
                        warn!(
                            model_config_id = record_id.as_str(),
                            error = err.as_str(),
                            "skipping invalid model config while listing model configs"
                        );
                        None
                    }
                }
            })
            .collect())
    }

    pub async fn get_model_config(&self, id: &str) -> Result<Option<ModelConfigRecord>, String> {
        self.normalized_model_config_by_id(id).await
    }

    #[cfg(test)]
    pub async fn upsert_chatos_model_config(
        &self,
        input: ChatosSyncedModelConfigRequest,
    ) -> Result<ModelConfigRecord, String> {
        validate_required("id", &input.id)?;
        validate_required("name", &input.name)?;
        validate_required("model", &input.model)?;
        if input.model_request_max_retries > 10 {
            return Err("model_request_max_retries must be between 0 and 10".to_string());
        }
        let provider = normalize_model_provider_input(&input.provider)?;
        let thinking_level =
            normalize_model_thinking_level_input(provider.as_str(), input.thinking_level)?;
        let prompt_vendor =
            normalize_model_prompt_vendor_input(input.prompt_vendor, provider.as_str())?;
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
            prompt_vendor,
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
            model_request_max_retries: input.model_request_max_retries,
            thinking_level,
            supports_images: input
                .supports_images
                .or_else(|| existing.as_ref().map(|item| item.supports_images))
                .unwrap_or(false),
            supports_reasoning: input
                .supports_reasoning
                .or_else(|| existing.as_ref().map(|item| item.supports_reasoning))
                .unwrap_or(false),
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
}
