// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::time::Duration;

use reqwest::Client;

use crate::config::AppConfig;
use crate::models::EngineModelProfile;

use super::super::protocol::{normalize_base_url, provider_supports_optional_thinking};
use super::AiClient;

pub(super) fn build_client_config(
    config: &AppConfig,
    profile: Option<&EngineModelProfile>,
) -> Result<AiClient, String> {
    let http = Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .build()
        .map_err(|err| err.to_string())?;
    let api_key = profile
        .and_then(|item| item.api_key.clone())
        .or_else(|| config.openai_api_key.clone());
    let base_url = profile
        .and_then(|item| item.base_url.clone())
        .unwrap_or_else(|| config.openai_base_url.clone());
    let model = profile
        .map(|item| item.model.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| config.openai_model.trim().to_string());
    let temperature = profile
        .and_then(|item| item.temperature)
        .unwrap_or(config.openai_temperature)
        .clamp(0.0, 2.0);
    let disable_thinking = provider_supports_optional_thinking(base_url.as_str(), model.as_str());

    Ok(AiClient {
        http,
        api_key,
        base_url: normalize_base_url(base_url.as_str()),
        model,
        temperature,
        timeout_secs: config.ai_request_timeout_secs,
        supports_responses: profile.map(|item| item.supports_responses).unwrap_or(false),
        disable_thinking,
        max_transient_retries: profile
            .map(|item| item.model_request_max_retries)
            .unwrap_or(5),
    })
}
