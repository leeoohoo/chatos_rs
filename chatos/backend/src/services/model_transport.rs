// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde_json::Value;

use crate::core::ai_model_config::ResolvedChatModelConfig;

pub fn model_transport_config_from_resolved(
    resolved: &ResolvedChatModelConfig,
) -> chatos_model_transport::ModelRuntimeConfig {
    chatos_model_transport::ModelRuntimeConfig::openai_compatible(
        resolved.base_url.clone(),
        resolved.api_key.clone(),
        resolved.model.clone(),
        resolved.provider.clone(),
    )
    .with_responses_support(resolved.supports_responses)
    .with_images_support(Some(resolved.supports_images))
    .with_temperature(Some(resolved.temperature))
    .with_thinking_level(resolved.thinking_level.clone())
    .with_instructions(resolved.system_prompt.clone())
    .with_max_transient_retries(Some(resolved.model_request_max_retries))
}

pub async fn resolve_model_transport_config_for_request(
    requested_model_config_id: Option<&str>,
    request_model_cfg: Option<&Value>,
    session_id: Option<&str>,
    user_id: Option<&str>,
    default_model: &str,
) -> Result<chatos_model_transport::ModelRuntimeConfig, String> {
    let resolved = crate::services::model_runtime_resolver::resolve_model_runtime_for_request(
        requested_model_config_id,
        request_model_cfg,
        session_id,
        user_id,
        default_model,
    )
    .await?;
    Ok(model_transport_config_from_resolved(&resolved))
}
