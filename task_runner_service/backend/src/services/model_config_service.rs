use chatos_ai_runtime::{
    build_responses_text_input, run_compatible_prompt_with, select_preferred_response_text,
    AiRequestHandler, SimplePromptOptions,
};
use tracing::{info, warn};
use uuid::Uuid;

use crate::models::{
    now_rfc3339, CreateModelConfigRequest, ModelCatalogResponse, ModelConfigRecord,
    ModelConfigTestResponse, ModelConfigUsageRecord, PreviewModelCatalogRequest,
    TestModelConfigRequest, UpdateModelConfigRequest,
};
use crate::store::AppStore;

use super::model_catalog::{
    fetch_model_catalog_for_record, normalize_model_base_url_input,
    normalize_model_config_record, normalize_model_provider_input,
    normalize_model_thinking_level_input,
};
use super::{normalized_optional, validate_required, ModelConfigService};

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

    pub async fn list_model_configs(&self) -> Result<Vec<ModelConfigRecord>, String> {
        let records = self.store.list_model_configs().await?;
        records
            .into_iter()
            .map(normalize_model_config_record)
            .collect::<Result<Vec<_>, _>>()
    }

    pub async fn get_model_config(&self, id: &str) -> Result<Option<ModelConfigRecord>, String> {
        self.store
            .get_model_config(id)
            .await?
            .map(normalize_model_config_record)
            .transpose()
    }

    pub async fn create_model_config(
        &self,
        input: CreateModelConfigRequest,
    ) -> Result<ModelConfigRecord, String> {
        validate_required("name", &input.name)?;
        validate_required("model", &input.model)?;
        let provider = normalize_model_provider_input(&input.provider)?;
        let thinking_level =
            normalize_model_thinking_level_input(provider.as_str(), input.thinking_level.clone())?;
        let now = now_rfc3339();
        let record = ModelConfigRecord {
            id: Uuid::new_v4().to_string(),
            name: input.name.trim().to_string(),
            provider: provider.clone(),
            base_url: normalize_model_base_url_input(provider.as_str(), Some(input.base_url)),
            api_key: input.api_key.trim().to_string(),
            model: input.model.trim().to_string(),
            usage_scenario: normalized_optional(input.usage_scenario),
            temperature: input.temperature,
            max_output_tokens: input.max_output_tokens,
            thinking_level,
            supports_responses: input
                .supports_responses
                .unwrap_or_else(|| provider == "openai"),
            instructions: normalized_optional(input.instructions),
            request_cwd: normalized_optional(input.request_cwd),
            include_prompt_cache_retention: input.include_prompt_cache_retention.unwrap_or(false),
            request_body_limit_bytes: input.request_body_limit_bytes,
            enabled: input.enabled.unwrap_or(true),
            created_at: now.clone(),
            updated_at: now,
        };
        self.store.save_model_config(record).await
    }

    pub async fn test_model_config(
        &self,
        id: &str,
        input: TestModelConfigRequest,
    ) -> Result<Option<ModelConfigTestResponse>, String> {
        let Some(model_config) = self
            .store
            .get_model_config(id)
            .await?
            .map(normalize_model_config_record)
            .transpose()?
        else {
            return Ok(None);
        };

        let prompt = input
            .prompt
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or("请简短回复：task runner model config test ok。");
        let runtime_config = model_config.to_runtime_config(None);
        let handler = AiRequestHandler::new();
        let tested_at = now_rfc3339();
        info!(
            model_config_id = model_config.id.as_str(),
            provider = model_config.provider.as_str(),
            model = model_config.model.as_str(),
            base_url = model_config.base_url.as_str(),
            supports_responses = model_config.supports_responses,
            prompt = prompt,
            "task runner test_model_config started"
        );

        let result = run_compatible_prompt_with(
            &handler,
            &runtime_config,
            prompt,
            SimplePromptOptions {
                temperature: model_config.temperature,
                max_output_tokens: model_config.max_output_tokens.or(Some(128)),
                ..SimplePromptOptions::default()
            },
            build_responses_text_input,
        )
        .await;

        let response = match result {
            Ok(ai_response) => {
                info!(
                    model_config_id = model_config.id.as_str(),
                    provider = model_config.provider.as_str(),
                    model = model_config.model.as_str(),
                    response_id = ai_response.response_id.as_deref().unwrap_or(""),
                    finish_content_chars = ai_response.content.chars().count(),
                    usage = ai_response
                        .usage
                        .as_ref()
                        .map(|value| value.to_string())
                        .unwrap_or_default(),
                    "task runner test_model_config succeeded"
                );
                ModelConfigTestResponse {
                    ok: true,
                    model_config_id: model_config.id.clone(),
                    provider: model_config.provider.clone(),
                    model: model_config.model.clone(),
                    content: select_preferred_response_text(
                        ai_response.content.as_str(),
                        ai_response.reasoning.as_deref(),
                    )
                    .map(ToOwned::to_owned),
                    reasoning: ai_response.reasoning,
                    usage: ai_response.usage,
                    response_id: ai_response.response_id,
                    error: None,
                    tested_at,
                }
            }
            Err(err) => {
                warn!(
                    model_config_id = model_config.id.as_str(),
                    provider = model_config.provider.as_str(),
                    model = model_config.model.as_str(),
                    error = err.as_str(),
                    "task runner test_model_config failed"
                );
                ModelConfigTestResponse {
                    ok: false,
                    model_config_id: model_config.id.clone(),
                    provider: model_config.provider.clone(),
                    model: model_config.model.clone(),
                    content: None,
                    reasoning: None,
                    usage: None,
                    response_id: None,
                    error: Some(err),
                    tested_at,
                }
            }
        };

        Ok(Some(response))
    }

    pub async fn update_model_config(
        &self,
        id: &str,
        patch: UpdateModelConfigRequest,
    ) -> Result<Option<ModelConfigRecord>, String> {
        let Some(mut model) = self.store.get_model_config(id).await? else {
            return Ok(None);
        };
        model = normalize_model_config_record(model)?;
        let original_provider = model.provider.clone();
        let original_base_url = model.base_url.clone();
        if let Some(name) = patch.name {
            validate_required("name", &name)?;
            model.name = name.trim().to_string();
        }
        if let Some(provider) = patch.provider {
            model.provider = normalize_model_provider_input(&provider)?;
        }
        if let Some(base_url) = patch.base_url {
            model.base_url =
                normalize_model_base_url_input(model.provider.as_str(), Some(base_url));
        } else if model.provider != original_provider
            && model.base_url
                == normalize_model_base_url_input(
                    original_provider.as_str(),
                    Some(original_base_url),
                )
        {
            model.base_url = normalize_model_base_url_input(model.provider.as_str(), None);
        }
        if let Some(api_key) = patch.api_key {
            model.api_key = api_key.trim().to_string();
        }
        if let Some(runtime_model) = patch.model {
            validate_required("model", &runtime_model)?;
            model.model = runtime_model.trim().to_string();
        }
        if let Some(usage_scenario) = patch.usage_scenario {
            model.usage_scenario = normalized_optional(Some(usage_scenario));
        }
        if let Some(temperature) = patch.temperature {
            model.temperature = Some(temperature);
        }
        if let Some(max_output_tokens) = patch.max_output_tokens {
            model.max_output_tokens = Some(max_output_tokens);
        }
        if let Some(thinking_level) = patch.thinking_level {
            model.thinking_level = normalize_model_thinking_level_input(
                model.provider.as_str(),
                Some(thinking_level),
            )?;
        }
        if let Some(supports_responses) = patch.supports_responses {
            model.supports_responses = supports_responses;
        }
        if let Some(instructions) = patch.instructions {
            model.instructions = normalized_optional(Some(instructions));
        }
        if let Some(request_cwd) = patch.request_cwd {
            model.request_cwd = normalized_optional(Some(request_cwd));
        }
        if let Some(include_prompt_cache_retention) = patch.include_prompt_cache_retention {
            model.include_prompt_cache_retention = include_prompt_cache_retention;
        }
        if let Some(request_body_limit_bytes) = patch.request_body_limit_bytes {
            model.request_body_limit_bytes = Some(request_body_limit_bytes);
        }
        if let Some(enabled) = patch.enabled {
            if !enabled {
                if let Some(task_id) = self.first_task_using_model_config(id).await? {
                    return Err(format!("模型配置仍被任务引用，暂时不能停用: {task_id}"));
                }
            }
            model.enabled = enabled;
        }
        model.thinking_level = normalize_model_thinking_level_input(
            model.provider.as_str(),
            model.thinking_level.clone(),
        )?;
        model.updated_at = now_rfc3339();
        Ok(Some(self.store.save_model_config(model).await?))
    }

    pub async fn list_model_catalog(
        &self,
        id: &str,
    ) -> Result<Option<ModelCatalogResponse>, String> {
        let Some(model) = self
            .store
            .get_model_config(id)
            .await?
            .map(normalize_model_config_record)
            .transpose()?
        else {
            return Ok(None);
        };
        info!(
            model_config_id = model.id.as_str(),
            provider = model.provider.as_str(),
            model = model.model.as_str(),
            base_url = model.base_url.as_str(),
            "task runner list_model_catalog started"
        );
        Ok(Some(
            fetch_model_catalog_for_record(Some(model.id.clone()), &model).await,
        ))
    }

    pub async fn preview_model_catalog(
        &self,
        input: PreviewModelCatalogRequest,
    ) -> Result<ModelCatalogResponse, String> {
        validate_required("provider", &input.provider)?;
        let provider = normalize_model_provider_input(&input.provider)?;
        let model = normalized_optional(input.model);
        let record = ModelConfigRecord {
            id: "preview".to_string(),
            name: "preview".to_string(),
            provider: provider.clone(),
            base_url: normalize_model_base_url_input(provider.as_str(), input.base_url),
            api_key: input
                .api_key
                .map(|value| value.trim().to_string())
                .unwrap_or_default(),
            model: model.unwrap_or_default(),
            usage_scenario: None,
            temperature: None,
            max_output_tokens: None,
            thinking_level: None,
            supports_responses: input
                .supports_responses
                .unwrap_or_else(|| provider == "openai"),
            instructions: None,
            request_cwd: None,
            include_prompt_cache_retention: false,
            request_body_limit_bytes: None,
            enabled: true,
            created_at: now_rfc3339(),
            updated_at: now_rfc3339(),
        };
        info!(
            provider = record.provider.as_str(),
            model = record.model.as_str(),
            base_url = record.base_url.as_str(),
            supports_responses = record.supports_responses,
            "task runner preview_model_catalog started"
        );
        Ok(fetch_model_catalog_for_record(None, &record).await)
    }

    pub async fn delete_model_config(&self, id: &str) -> Result<bool, String> {
        self.store.delete_model_config(id).await
    }

    pub async fn usage_stats(&self) -> Result<Vec<ModelConfigUsageRecord>, String> {
        self.store.list_model_config_usage().await
    }
}
