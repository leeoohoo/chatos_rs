// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_ai_runtime::ModelRuntimeConfig;
use chatos_service_runtime::{
    resolve_local_connector_model_runtime, LocalConnectorModelRuntimeLookup,
};
use serde::Deserialize;

use crate::config::AppConfig;
use crate::http_body::{read_response_text_limited_or_message, ERROR_BODY_PREVIEW_LIMIT_BYTES};

pub(crate) const HARNESS_REPO_WRITE_SCOPE: &str = "harness.repo.write";
pub(crate) const HARNESS_ACCESS_READ_SCOPE: &str = "harness.access.read";
const MODEL_SETTINGS_READ_SCOPE: &str = "model-settings.read";

#[derive(Debug, Clone, Deserialize)]
struct UserServiceModelSettingsResponse {
    project_management_agent_model_config_id: Option<String>,
    project_management_agent_thinking_level: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ProjectAgentModelSettings {
    pub model_config_id: Option<String>,
    pub thinking_level: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ResolvedProjectAgentModelRuntime {
    pub model_config_id: String,
    pub model_config: ModelRuntimeConfig,
}

pub async fn get_project_agent_model_settings(
    config: &AppConfig,
    owner_user_id: &str,
) -> Result<ProjectAgentModelSettings, String> {
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
    let client = reqwest::Client::builder()
        .timeout(config.user_service_request_timeout)
        .build()
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
    let record = response
        .json::<UserServiceModelSettingsResponse>()
        .await
        .map_err(|err| format!("parse user_service model settings response failed: {err}"))?;
    Ok(ProjectAgentModelSettings {
        model_config_id: normalized_optional(record.project_management_agent_model_config_id),
        thinking_level: normalized_optional(record.project_management_agent_thinking_level),
    })
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

pub async fn resolve_default_project_agent_model_runtime(
    config: &AppConfig,
    owner_user_id: &str,
) -> Result<Option<ResolvedProjectAgentModelRuntime>, String> {
    let settings = get_project_agent_model_settings(config, owner_user_id).await?;
    let Some(model_config_id) = settings.model_config_id.as_deref() else {
        return Ok(None);
    };
    resolve_project_agent_model_runtime(
        config,
        owner_user_id,
        model_config_id,
        settings.thinking_level.as_deref(),
    )
    .await
    .map(Some)
}

pub async fn resolve_project_agent_model_runtime(
    config: &AppConfig,
    owner_user_id: &str,
    model_config_id: &str,
    thinking_level_override: Option<&str>,
) -> Result<ResolvedProjectAgentModelRuntime, String> {
    let owner_user_id = owner_user_id.trim();
    let model_config_id = model_config_id.trim();
    if owner_user_id.is_empty() {
        return Err("owner_user_id is required".to_string());
    }
    if model_config_id.is_empty() {
        return Err("project management agent model_config_id is required".to_string());
    }
    let secret = local_connector_internal_secret()?;
    let record = resolve_local_connector_model_runtime(LocalConnectorModelRuntimeLookup {
        base_url: config.local_connector_service_base_url.as_str(),
        request_timeout: config.local_connector_service_request_timeout,
        internal_secret: secret.as_str(),
        caller: "project-service",
        owner_user_id,
        model_config_id,
    })
    .await
    .map_err(|err| err.to_string())?;
    let thinking_level = normalized_optional(thinking_level_override.map(ToOwned::to_owned))
        .or_else(|| normalized_optional(record.thinking_level));
    let provider = runtime_provider_for_model(record.provider.as_str(), record.base_url.as_str());
    let model_config = ModelRuntimeConfig::openai_compatible(
        record.base_url,
        record.api_key,
        record.model,
        provider,
    )
    .with_responses_support(record.supports_responses)
    .with_images_support(Some(record.supports_images))
    .with_thinking_level(thinking_level);

    Ok(ResolvedProjectAgentModelRuntime {
        model_config_id: record.id,
        model_config,
    })
}

fn user_service_internal_secret(config: &AppConfig) -> Result<&str, String> {
    config
        .user_service_internal_secret
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "PROJECT_SERVICE_USER_SERVICE_INTERNAL_SECRET is not configured".to_string())
}

fn local_connector_internal_secret() -> Result<String, String> {
    let secret = std::env::var("PROJECT_SERVICE_LOCAL_CONNECTOR_INTERNAL_API_SECRET")
        .ok()
        .or_else(|| std::env::var("LOCAL_CONNECTOR_INTERNAL_API_SECRET").ok())
        .or_else(|| std::env::var("CHATOS_LOCAL_CONNECTOR_INTERNAL_API_SECRET").ok())
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            "PROJECT_SERVICE_LOCAL_CONNECTOR_INTERNAL_API_SECRET is required to resolve local model runtime"
                .to_string()
        })?;
    chatos_service_runtime::validate_production_secret(
        "PROJECT_SERVICE_LOCAL_CONNECTOR_INTERNAL_API_SECRET",
        Some(secret.as_str()),
        &[
            "chatos-local-connector-dev-secret",
            "change_me_project_service_local_connector_secret",
        ],
    )?;
    Ok(secret)
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
