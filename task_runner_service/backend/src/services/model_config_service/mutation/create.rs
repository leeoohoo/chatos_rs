// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

impl ModelConfigService {
    pub async fn create_model_config(
        &self,
        input: CreateModelConfigRequest,
    ) -> Result<ModelConfigRecord, String> {
        validate_required("name", &input.name)?;
        validate_required("model", &input.model)?;
        let provider = normalize_model_provider_input(&input.provider)?;
        let thinking_level =
            normalize_model_thinking_level_input(provider.as_str(), input.thinking_level.clone())?;
        let prompt_vendor =
            normalize_model_prompt_vendor_input(input.prompt_vendor, provider.as_str())?;
        let now = now_rfc3339();
        let record = ModelConfigRecord {
            id: Uuid::new_v4().to_string(),
            owner_user_id: None,
            owner_username: None,
            owner_display_name: None,
            name: input.name.trim().to_string(),
            provider: provider.clone(),
            prompt_vendor,
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
}
