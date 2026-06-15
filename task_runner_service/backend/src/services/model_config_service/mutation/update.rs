use super::*;

impl ModelConfigService {
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
}
