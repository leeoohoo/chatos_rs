// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_ai_runtime::ModelRuntimeConfig;
use chatos_plugin_management_sdk::normalize_agent_prompt_vendor;
use chatos_service_runtime::{build_http_client, HttpClientTimeouts};
use serde::Deserialize;

use crate::config::AppConfig;
use crate::http_body::{
    read_response_json_limited, read_response_text_limited_or_message,
    ERROR_BODY_PREVIEW_LIMIT_BYTES, JSON_BODY_LIMIT_BYTES,
};

pub(crate) const HARNESS_REPO_WRITE_SCOPE: &str = "harness.repo.write";
pub(crate) const HARNESS_ACCESS_READ_SCOPE: &str = "harness.access.read";
const MODEL_SETTINGS_READ_SCOPE: &str = "model-settings.read";
const MODEL_RUNTIME_READ_SCOPE: &str = "model-runtime.read";

#[derive(Debug, Clone, Deserialize)]
struct UserServiceModelSettingsResponse {
    environment_initialization_model_config_id: Option<String>,
    environment_initialization_thinking_level: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct UserServiceModelRuntimeResponse {
    id: String,
    provider: String,
    #[serde(default)]
    prompt_vendor: Option<String>,
    base_url: String,
    api_key: String,
    model: String,
    thinking_level: Option<String>,
    #[serde(default)]
    supports_images: bool,
    #[serde(default)]
    supports_responses: bool,
}

#[derive(Debug, Clone)]
pub struct EnvironmentInitializationModelSettings {
    pub model_config_id: Option<String>,
    pub thinking_level: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ResolvedEnvironmentInitializationModelRuntime {
    pub model_config_id: String,
    pub prompt_vendor: Option<String>,
    pub model_config: ModelRuntimeConfig,
}

pub async fn get_environment_initialization_model_settings(
    config: &AppConfig,
    owner_user_id: &str,
) -> Result<EnvironmentInitializationModelSettings, String> {
    let owner_user_id = owner_user_id.trim();
    if owner_user_id.is_empty() {
        return Err("owner_user_id is required".to_string());
    }
    let secret = user_service_internal_secret(config)?;
    let endpoint = format!(
        "{}/api/internal/users/{}/model-settings",
        config.user_service_base_url.trim().trim_end_matches('/'),
        urlencoding::encode(owner_user_id)
    );
    let client = build_http_client(HttpClientTimeouts::new(config.user_service_request_timeout))
        .map_err(|err| format!("build user_service client failed: {err}"))?;
    let response =
        signed_user_service_request(client.get(endpoint), secret, MODEL_SETTINGS_READ_SCOPE)?
            .send()
            .await
            .map_err(|err| format!("user_service model settings request failed: {err}"))?;
    let status = response.status();
    if !status.is_success() {
        let body =
            read_response_text_limited_or_message(response, ERROR_BODY_PREVIEW_LIMIT_BYTES).await;
        return Err(if body.trim().is_empty() {
            format!("user_service model settings request failed with status {status}")
        } else {
            body
        });
    }
    let record = read_response_json_limited::<UserServiceModelSettingsResponse>(
        response,
        JSON_BODY_LIMIT_BYTES,
    )
    .await
    .map_err(|err| format!("parse user_service model settings response failed: {err}"))?;
    Ok(environment_initialization_settings_from_response(record))
}

fn environment_initialization_settings_from_response(
    record: UserServiceModelSettingsResponse,
) -> EnvironmentInitializationModelSettings {
    EnvironmentInitializationModelSettings {
        model_config_id: normalized_optional(record.environment_initialization_model_config_id),
        thinking_level: normalized_optional(record.environment_initialization_thinking_level),
    }
}

pub(crate) fn signed_user_service_request(
    request: reqwest::RequestBuilder,
    internal_secret: &str,
    scope: &str,
) -> Result<reqwest::RequestBuilder, String> {
    let internal_secret = internal_secret.trim();
    let token = chatos_service_runtime::issue_internal_service_token(
        internal_secret,
        "project-service",
        "user-service",
        scope,
        60,
    )?;
    Ok(request
        .header("X-User-Service-Caller", "project-service")
        .header("X-User-Service-Internal-Token", token))
}

pub async fn resolve_default_environment_initialization_model_runtime(
    config: &AppConfig,
    owner_user_id: &str,
) -> Result<Option<ResolvedEnvironmentInitializationModelRuntime>, String> {
    let settings = get_environment_initialization_model_settings(config, owner_user_id).await?;
    let Some(model_config_id) = settings.model_config_id.as_deref() else {
        return Ok(None);
    };
    resolve_environment_initialization_model_runtime(
        config,
        owner_user_id,
        model_config_id,
        settings.thinking_level.as_deref(),
    )
    .await
    .map(Some)
}

pub async fn resolve_environment_initialization_model_runtime(
    config: &AppConfig,
    owner_user_id: &str,
    model_config_id: &str,
    thinking_level_override: Option<&str>,
) -> Result<ResolvedEnvironmentInitializationModelRuntime, String> {
    let owner_user_id = owner_user_id.trim();
    let model_config_id = model_config_id.trim();
    if owner_user_id.is_empty() {
        return Err("owner_user_id is required".to_string());
    }
    if model_config_id.is_empty() {
        return Err("environment initialization model_config_id is required".to_string());
    }
    let record = get_cloud_model_runtime(config, owner_user_id, model_config_id).await?;
    resolve_environment_initialization_model_runtime_from_response(record, thinking_level_override)
}

fn resolve_environment_initialization_model_runtime_from_response(
    record: UserServiceModelRuntimeResponse,
    thinking_level_override: Option<&str>,
) -> Result<ResolvedEnvironmentInitializationModelRuntime, String> {
    let thinking_level = normalized_optional(thinking_level_override.map(ToOwned::to_owned))
        .or_else(|| normalized_optional(record.thinking_level));
    let configured_provider = normalize_configured_provider(record.provider.as_str())?;
    let prompt_vendor = normalized_optional(record.prompt_vendor).or_else(|| {
        normalize_agent_prompt_vendor(None, configured_provider.as_str())
            .map(|vendor| vendor.as_str().to_string())
    });
    let provider =
        runtime_provider_for_model(configured_provider.as_str(), record.base_url.as_str());
    let model_config = ModelRuntimeConfig::openai_compatible(
        record.base_url,
        record.api_key,
        record.model,
        provider,
    )
    .with_responses_support(record.supports_responses)
    .with_images_support(Some(record.supports_images))
    .with_thinking_level(thinking_level);

    Ok(ResolvedEnvironmentInitializationModelRuntime {
        model_config_id: record.id,
        prompt_vendor,
        model_config,
    })
}

fn normalize_configured_provider(provider: &str) -> Result<String, String> {
    let normalized = provider.trim().to_ascii_lowercase().replace('-', "_");
    let provider = match normalized.as_str() {
        "openai" | "gpt" => "gpt",
        "deepseek" => "deepseek",
        "kimi" | "kimik2" | "moonshot" => "kimi",
        "glm" | "zhipu" | "zhipuai" | "zai" | "chatglm" => "glm",
        _ => return Err("provider only supports gpt / deepseek / kimi / glm".to_string()),
    };
    Ok(provider.to_string())
}

fn user_service_internal_secret(config: &AppConfig) -> Result<&str, String> {
    config
        .user_service_internal_secret
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "PROJECT_SERVICE_USER_SERVICE_INTERNAL_SECRET is not configured".to_string())
}

async fn get_cloud_model_runtime(
    config: &AppConfig,
    owner_user_id: &str,
    model_config_id: &str,
) -> Result<UserServiceModelRuntimeResponse, String> {
    let secret = user_service_internal_secret(config)?;
    let endpoint = format!(
        "{}/api/internal/users/{}/model-configs/{}/runtime",
        config.user_service_base_url.trim().trim_end_matches('/'),
        urlencoding::encode(owner_user_id),
        urlencoding::encode(model_config_id),
    );
    let client = build_http_client(HttpClientTimeouts::new(config.user_service_request_timeout))
        .map_err(|err| format!("build user_service client failed: {err}"))?;
    let response =
        signed_user_service_request(client.get(endpoint), secret, MODEL_RUNTIME_READ_SCOPE)?
            .send()
            .await
            .map_err(|err| format!("user_service model runtime request failed: {err}"))?;
    let status = response.status();
    if !status.is_success() {
        let body =
            read_response_text_limited_or_message(response, ERROR_BODY_PREVIEW_LIMIT_BYTES).await;
        return Err(if body.trim().is_empty() {
            format!("user_service model runtime request failed with status {status}")
        } else {
            body
        });
    }
    read_response_json_limited::<UserServiceModelRuntimeResponse>(response, JSON_BODY_LIMIT_BYTES)
        .await
        .map_err(|err| format!("parse user_service model runtime response failed: {err}"))
}

fn runtime_provider_for_model(provider: &str, base_url: &str) -> String {
    let normalized = provider.trim().to_ascii_lowercase().replace('-', "_");
    if matches!(normalized.as_str(), "openai" | "gpt") && !is_openai_api_base_url(base_url) {
        return "openai_compatible".to_string();
    }
    match normalized.as_str() {
        "openai" => "gpt".to_string(),
        "" => "gpt".to_string(),
        other => other.to_string(),
    }
}

fn is_openai_api_base_url(base_url: &str) -> bool {
    let value = base_url.trim().to_ascii_lowercase();
    value.is_empty() || value.contains("api.openai.com")
}

fn normalized_optional(value: Option<String>) -> Option<String> {
    value
        .map(|item| item.trim().to_string())
        .filter(|item| !item.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn environment_initialization_uses_its_own_default_model() {
        let record =
            serde_json::from_value::<UserServiceModelSettingsResponse>(serde_json::json!({
                "project_management_agent_model_config_id": "project-model",
                "project_management_agent_thinking_level": "medium",
                "environment_initialization_model_config_id": "environment-model",
                "environment_initialization_thinking_level": "high"
            }))
            .expect("settings response");

        let settings = environment_initialization_settings_from_response(record);

        assert_eq!(
            settings.model_config_id.as_deref(),
            Some("environment-model")
        );
        assert_eq!(settings.thinking_level.as_deref(), Some("high"));
    }

    #[test]
    fn custom_openai_gateway_keeps_gpt_prompt_vendor() {
        let runtime = resolve_environment_initialization_model_runtime_from_response(
            UserServiceModelRuntimeResponse {
                id: "model-1".to_string(),
                provider: "gpt".to_string(),
                prompt_vendor: None,
                base_url: "https://gateway.example.invalid/v1".to_string(),
                api_key: "secret".to_string(),
                model: "gpt-compatible".to_string(),
                thinking_level: Some("high".to_string()),
                supports_images: false,
                supports_responses: true,
            },
            None,
        )
        .expect("runtime");

        assert_eq!(runtime.prompt_vendor.as_deref(), Some("gpt"));
        assert_eq!(runtime.model_config.provider, "openai_compatible");
    }

    #[test]
    fn glm_runtime_keeps_glm_prompt_vendor() {
        let runtime = resolve_environment_initialization_model_runtime_from_response(
            UserServiceModelRuntimeResponse {
                id: "model-1".to_string(),
                provider: "glm".to_string(),
                prompt_vendor: None,
                base_url: "https://open.bigmodel.cn/api/paas/v4".to_string(),
                api_key: "secret".to_string(),
                model: "glm-4-plus".to_string(),
                thinking_level: Some("high".to_string()),
                supports_images: false,
                supports_responses: false,
            },
            None,
        )
        .expect("runtime");

        assert_eq!(runtime.prompt_vendor.as_deref(), Some("glm"));
        assert_eq!(runtime.model_config.provider, "glm");
    }

    #[test]
    fn removed_provider_values_are_rejected() {
        for provider in ["openai_compatible", "minimax"] {
            let result = resolve_environment_initialization_model_runtime_from_response(
                UserServiceModelRuntimeResponse {
                    id: "model-1".to_string(),
                    provider: provider.to_string(),
                    prompt_vendor: None,
                    base_url: "https://removed.example.invalid/v1".to_string(),
                    api_key: "secret".to_string(),
                    model: "removed-model".to_string(),
                    thinking_level: None,
                    supports_images: false,
                    supports_responses: false,
                },
                None,
            );

            assert!(result.is_err());
        }
    }

    #[test]
    fn signed_user_service_request_uses_scoped_token_without_static_secret() {
        let request = signed_user_service_request(
            reqwest::Client::new().get("http://127.0.0.1:39190/api/internal/test"),
            "a-long-project-user-service-secret",
            HARNESS_ACCESS_READ_SCOPE,
        )
        .expect("signed request")
        .build()
        .expect("build request");
        assert_eq!(
            request
                .headers()
                .get("x-user-service-caller")
                .and_then(|value| value.to_str().ok()),
            Some("project-service")
        );
        let token = request
            .headers()
            .get("x-user-service-internal-token")
            .and_then(|value| value.to_str().ok())
            .expect("internal token");
        chatos_service_runtime::verify_internal_service_token(
            token,
            "a-long-project-user-service-secret",
            "project-service",
            "user-service",
            HARNESS_ACCESS_READ_SCOPE,
        )
        .expect("valid token");
        assert!(!request
            .headers()
            .contains_key("x-user-service-internal-secret"));
    }
}
