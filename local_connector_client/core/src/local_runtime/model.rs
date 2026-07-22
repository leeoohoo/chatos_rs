// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_ai_runtime::ModelRuntimeConfig;

use crate::model_configs::LocalModelRuntimeResponse;

pub(crate) fn build_local_model_config(
    runtime: LocalModelRuntimeResponse,
    system_prompt: Option<String>,
    thinking_level: Option<String>,
    temperature: Option<f64>,
    reasoning_enabled: bool,
    workspace_root: Option<String>,
) -> ModelRuntimeConfig {
    ModelRuntimeConfig::openai_compatible(
        runtime.base_url,
        runtime.api_key,
        runtime.model,
        runtime.provider,
    )
    .with_responses_support(runtime.supports_responses)
    .with_images_support(Some(runtime.supports_images))
    .with_instructions(system_prompt)
    .with_temperature(temperature.or(runtime.temperature))
    .with_max_output_tokens(runtime.max_output_tokens)
    .with_max_transient_retries(Some(runtime.model_request_max_retries))
    .with_thinking_level(if reasoning_enabled && runtime.supports_reasoning {
        thinking_level.or(runtime.thinking_level)
    } else {
        None
    })
    .with_request_cwd(workspace_root)
}

#[cfg(test)]
mod tests {
    use crate::model_configs::LocalModelRuntimeResponse;

    use super::build_local_model_config;

    #[test]
    fn disables_thinking_when_reasoning_is_not_enabled() {
        let config = build_local_model_config(
            LocalModelRuntimeResponse {
                id: "model-1".to_string(),
                local_model_config_id: "local-model-1".to_string(),
                provider: "openai".to_string(),
                prompt_vendor: Some("gpt".to_string()),
                base_url: "https://example.invalid/v1".to_string(),
                api_key: "secret".to_string(),
                model: "demo".to_string(),
                thinking_level: Some("high".to_string()),
                supports_images: false,
                supports_reasoning: true,
                supports_responses: true,
                temperature: Some(0.5),
                max_output_tokens: Some(512),
                model_request_max_retries: 5,
            },
            None,
            Some("medium".to_string()),
            None,
            false,
            None,
        );
        assert_eq!(config.thinking_level, None);
        assert_eq!(config.temperature, Some(0.5));
        assert_eq!(config.max_transient_retries, Some(5));
    }
}
