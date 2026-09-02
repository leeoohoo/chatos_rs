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
    let profile = profile.ok_or_else(|| {
        "User Service runtime model is required for Memory Engine AI execution".to_string()
    })?;
    let http = Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .build()
        .map_err(|err| err.to_string())?;
    let api_key = profile.api_key.clone();
    let base_url = profile
        .base_url
        .clone()
        .ok_or_else(|| "User Service runtime model is missing base_url".to_string())?;
    let model = profile.model.trim().to_string();
    if model.is_empty() {
        return Err("User Service runtime model is missing model name".to_string());
    }
    let temperature = profile.temperature.unwrap_or(0.2).clamp(0.0, 2.0);
    let disable_thinking = provider_supports_optional_thinking(base_url.as_str(), model.as_str());

    Ok(AiClient {
        http,
        api_key,
        base_url: normalize_base_url(base_url.as_str()),
        model,
        temperature,
        timeout_secs: config.ai_request_timeout_secs,
        supports_responses: profile.supports_responses,
        disable_thinking,
        max_transient_retries: profile.model_request_max_retries,
    })
}
