// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::OnceLock;
use std::time::Duration;

use chatos_service_runtime::http_body::{
    read_response_json_limited, read_response_preview_text_limited_or_message,
    ERROR_BODY_PREVIEW_LIMIT_BYTES, JSON_BODY_LIMIT_BYTES,
};
use reqwest::{Client, Method};
use serde::Deserialize;

use crate::config::AppConfig;
use crate::models::EngineModelProfile;

const USER_SERVICE_CALLER: &str = "memory-engine";
const USER_SERVICE_AUDIENCE: &str = "user-service";
const MODEL_SETTINGS_READ_SCOPE: &str = "model-settings.read";
const MODEL_RUNTIME_READ_SCOPE: &str = "model-runtime.read";

#[derive(Debug, Deserialize)]
struct UserModelSettings {
    model_request_max_retries: i64,
    memory_summary_model_config_id: Option<String>,
    memory_summary_thinking_level: Option<String>,
}

#[derive(Debug, Deserialize)]
struct UserModelRuntime {
    id: String,
    owner_user_id: String,
    name: String,
    provider: String,
    base_url: String,
    api_key: String,
    model: String,
    thinking_level: Option<String>,
    temperature: Option<f64>,
    supports_images: bool,
    supports_reasoning: bool,
    supports_responses: bool,
}

pub(super) async fn resolve_memory_summary_model_runtime(
    config: &AppConfig,
    owner_user_id: &str,
) -> Result<EngineModelProfile, String> {
    let owner_user_id = required_owner_user_id(owner_user_id)?;
    let settings: UserModelSettings = request_user_service_json(
        config,
        MODEL_SETTINGS_READ_SCOPE,
        format!(
            "/api/internal/users/{}/model-settings",
            urlencoding::encode(owner_user_id)
        )
        .as_str(),
    )
    .await?;
    let model_config_id = settings
        .memory_summary_model_config_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            format!(
                "memory_summary_model_not_configured: user {owner_user_id} has no Memory Engine summary model selected"
            )
        })?;
    let runtime = load_model_runtime(config, owner_user_id, model_config_id).await?;
    Ok(runtime.into_profile(
        settings.memory_summary_thinking_level,
        normalize_retries(settings.model_request_max_retries),
        true,
    ))
}

pub(super) async fn resolve_model_runtime_by_id(
    config: &AppConfig,
    owner_user_id: &str,
    model_config_id: &str,
) -> Result<EngineModelProfile, String> {
    let owner_user_id = required_owner_user_id(owner_user_id)?;
    let model_config_id = model_config_id.trim();
    if model_config_id.is_empty() {
        return Err("model_config_id is required".to_string());
    }
    let settings: UserModelSettings = request_user_service_json(
        config,
        MODEL_SETTINGS_READ_SCOPE,
        format!(
            "/api/internal/users/{}/model-settings",
            urlencoding::encode(owner_user_id)
        )
        .as_str(),
    )
    .await?;
    let runtime = load_model_runtime(config, owner_user_id, model_config_id).await?;
    let is_memory_summary_default = settings
        .memory_summary_model_config_id
        .as_deref()
        .is_some_and(|selected| selected.trim() == model_config_id);
    let thinking_level = if is_memory_summary_default {
        settings.memory_summary_thinking_level
    } else {
        None
    };
    Ok(runtime.into_profile(
        thinking_level,
        normalize_retries(settings.model_request_max_retries),
        is_memory_summary_default,
    ))
}

async fn load_model_runtime(
    config: &AppConfig,
    owner_user_id: &str,
    model_config_id: &str,
) -> Result<UserModelRuntime, String> {
    request_user_service_json(
        config,
        MODEL_RUNTIME_READ_SCOPE,
        format!(
            "/api/internal/users/{}/model-configs/{}/runtime",
            urlencoding::encode(owner_user_id),
            urlencoding::encode(model_config_id)
        )
        .as_str(),
    )
    .await
}

async fn request_user_service_json<T>(
    config: &AppConfig,
    scope: &str,
    path: &str,
) -> Result<T, String>
where
    T: serde::de::DeserializeOwned,
{
    let secret = config
        .internal_api_secrets
        .get("user-service")
        .map(String::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            "USER_SERVICE_MEMORY_ENGINE_INTERNAL_API_SECRET is not configured".to_string()
        })?;
    let token = chatos_service_runtime::issue_internal_service_token(
        secret,
        USER_SERVICE_CALLER,
        USER_SERVICE_AUDIENCE,
        scope,
        60,
    )?;
    let endpoint = format!(
        "{}{}",
        config.user_service_base_url.trim().trim_end_matches('/'),
        path
    );
    let response = user_service_client()
        .request(Method::GET, endpoint)
        .timeout(Duration::from_millis(
            config.user_service_request_timeout_ms.max(300),
        ))
        .header("x-user-service-caller", USER_SERVICE_CALLER)
        .header("x-user-service-internal-token", token)
        .send()
        .await
        .map_err(|error| format!("request user_service model runtime failed: {error}"))?;
    let status = response.status();
    if !status.is_success() {
        let body =
            read_response_preview_text_limited_or_message(response, ERROR_BODY_PREVIEW_LIMIT_BYTES)
                .await;
        return Err(format!(
            "user_service model runtime request failed: {} {}",
            status.as_u16(),
            body
        ));
    }
    read_response_json_limited::<T>(response, JSON_BODY_LIMIT_BYTES)
        .await
        .map_err(|error| format!("parse user_service model runtime failed: {error}"))
}

fn user_service_client() -> &'static Client {
    static CLIENT: OnceLock<Client> = OnceLock::new();
    CLIENT.get_or_init(Client::new)
}

fn required_owner_user_id(owner_user_id: &str) -> Result<&str, String> {
    let owner_user_id = owner_user_id.trim();
    if owner_user_id.is_empty() || owner_user_id == "system" {
        return Err("owner_user_id is required to resolve a user model at runtime".to_string());
    }
    Ok(owner_user_id)
}

fn normalize_retries(value: i64) -> usize {
    usize::try_from(value.clamp(0, 10)).unwrap_or(5)
}

fn memory_engine_provider(provider: &str) -> String {
    match provider.trim() {
        "deepseek" => "deepseek".to_string(),
        _ => "openai".to_string(),
    }
}

impl UserModelRuntime {
    fn into_profile(
        self,
        thinking_level_override: Option<String>,
        model_request_max_retries: usize,
        is_default: bool,
    ) -> EngineModelProfile {
        EngineModelProfile {
            id: self.id,
            owner_user_id: Some(self.owner_user_id),
            owner_username: None,
            name: self.name,
            provider: memory_engine_provider(self.provider.as_str()),
            model: self.model,
            base_url: Some(self.base_url),
            api_key: Some(self.api_key),
            supports_images: self.supports_images,
            supports_reasoning: self.supports_reasoning,
            supports_responses: self.supports_responses,
            temperature: self.temperature,
            thinking_level: thinking_level_override.or(self.thinking_level),
            model_request_max_retries,
            is_default,
            enabled: true,
            created_at: String::new(),
            updated_at: String::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtime_profile_uses_user_settings_override_without_task_runner_state() {
        let profile = UserModelRuntime {
            id: "model-1".to_string(),
            owner_user_id: "user-1".to_string(),
            name: "Summary model".to_string(),
            provider: "gpt".to_string(),
            base_url: "https://example.com/v1".to_string(),
            api_key: "secret".to_string(),
            model: "gpt-test".to_string(),
            thinking_level: Some("low".to_string()),
            temperature: Some(0.2),
            supports_images: false,
            supports_reasoning: true,
            supports_responses: true,
        }
        .into_profile(Some("high".to_string()), 3, true);

        assert_eq!(profile.owner_user_id.as_deref(), Some("user-1"));
        assert_eq!(profile.thinking_level.as_deref(), Some("high"));
        assert_eq!(profile.model_request_max_retries, 3);
        assert!(profile.is_default);
    }
}
