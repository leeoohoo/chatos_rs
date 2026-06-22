use super::*;

impl ModelConfigService {
    pub async fn test_model_config(
        &self,
        id: &str,
        input: TestModelConfigRequest,
    ) -> Result<Option<ModelConfigTestResponse>, String> {
        let Some(model_config) = self.normalized_model_config_by_id(id).await? else {
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
}
