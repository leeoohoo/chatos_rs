use super::*;

impl ModelConfigService {
    pub async fn list_model_catalog(
        &self,
        id: &str,
    ) -> Result<Option<ModelCatalogResponse>, String> {
        let Some(model) = self.normalized_model_config_by_id(id).await? else {
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
}
