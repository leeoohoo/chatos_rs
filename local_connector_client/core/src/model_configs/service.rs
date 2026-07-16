// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use anyhow::{anyhow, Context, Result};
use reqwest::Method;
use serde::de::DeserializeOwned;
use serde_json::{json, Value};
use std::collections::HashSet;
use uuid::Uuid;

use crate::config::{api_url, normalize_optional};
use crate::relay::{relay_error_response, RelayRequest};
use crate::{local_now_rfc3339, AuthState, LocalState};

use super::provider_catalog::{
    default_base_url_for_provider, fallback_model_list, fetch_provider_models, normalize_provider,
    runtime_provider_for_model,
};
use super::types::{
    LocalModelCatalogResponse, LocalModelConfigDraft, LocalModelConfigPublic,
    LocalModelConfigRecord, LocalModelRuntimeResponse, LocalModelSettings,
};

pub(crate) fn list_local_model_configs(state: &LocalState) -> Vec<LocalModelConfigPublic> {
    state
        .model_configs
        .configs
        .iter()
        .map(LocalModelConfigRecord::public_value)
        .collect()
}

pub(crate) async fn preview_local_model_catalog(
    http_client: &reqwest::Client,
    state: &LocalState,
    draft: LocalModelConfigDraft,
) -> Result<LocalModelCatalogResponse> {
    let provider = normalize_provider(draft.provider.clone());
    let existing = draft
        .id
        .as_deref()
        .and_then(|id| normalize_optional(Some(id)))
        .and_then(|id| {
            state
                .model_configs
                .configs
                .iter()
                .find(|item| item.id == id)
        })
        .cloned();
    let base_url = normalize_optional(draft.base_url.as_deref())
        .or_else(|| {
            existing
                .as_ref()
                .and_then(|item| normalize_optional(item.base_url.as_deref()))
        })
        .unwrap_or_else(|| default_base_url_for_provider(provider.as_str()));
    let api_key = if draft.clear_api_key.unwrap_or(false) {
        None
    } else {
        normalize_optional(draft.api_key.as_deref()).or_else(|| {
            existing
                .as_ref()
                .and_then(|item| normalize_optional(item.api_key.as_deref()))
        })
    };
    let fallback_model = normalize_optional(draft.model.as_deref()).or_else(|| {
        existing
            .as_ref()
            .and_then(|item| normalize_optional(Some(item.model.as_str())))
    });
    let Some(api_key) = api_key else {
        return Ok(LocalModelCatalogResponse {
            provider,
            base_url,
            source: "fallback".to_string(),
            fetched_at: None,
            models: fallback_model_list(fallback_model.as_deref()),
            error: Some("当前供应商配置未提供 API Key".to_string()),
        });
    };

    match fetch_provider_models(
        http_client,
        provider.as_str(),
        base_url.as_str(),
        api_key.as_str(),
    )
    .await
    {
        Ok(models) => Ok(LocalModelCatalogResponse {
            provider,
            base_url,
            source: "live".to_string(),
            fetched_at: Some(local_now_rfc3339()),
            models,
            error: None,
        }),
        Err(error) => Ok(LocalModelCatalogResponse {
            provider,
            base_url,
            source: "fallback".to_string(),
            fetched_at: None,
            models: fallback_model_list(fallback_model.as_deref()),
            error: Some(error),
        }),
    }
}

pub(crate) fn save_local_model_config(
    state: &mut LocalState,
    draft: LocalModelConfigDraft,
) -> Result<LocalModelConfigRecord> {
    let name = required_text(Some(draft.name), "name")?;
    let model = required_text(draft.model, "model")?;
    let provider = normalize_provider(draft.provider);
    let now = local_now_rfc3339();
    let id = draft
        .id
        .as_deref()
        .and_then(|value| normalize_optional(Some(value)))
        .unwrap_or_else(|| Uuid::new_v4().to_string());
    let existing_index = state
        .model_configs
        .configs
        .iter()
        .position(|item| item.id == id);
    let existing = existing_index.and_then(|idx| state.model_configs.configs.get(idx).cloned());
    let copied_api_key = draft
        .copy_api_key_from_id
        .as_deref()
        .and_then(|copy_id| normalize_optional(Some(copy_id)))
        .and_then(|copy_id| {
            state
                .model_configs
                .configs
                .iter()
                .find(|item| item.id == copy_id)
        })
        .and_then(|item| normalize_optional(item.api_key.as_deref()));
    let api_key = if draft.clear_api_key.unwrap_or(false) {
        None
    } else {
        normalize_optional(draft.api_key.as_deref())
            .or_else(|| {
                existing
                    .as_ref()
                    .and_then(|item| normalize_optional(item.api_key.as_deref()))
            })
            .or(copied_api_key)
    };
    let record = LocalModelConfigRecord {
        id,
        server_model_config_id: normalize_optional(draft.server_model_config_id.as_deref())
            .or_else(|| {
                existing
                    .as_ref()
                    .and_then(|item| item.server_model_config_id.clone())
            }),
        name,
        provider,
        model,
        base_url: normalize_optional(draft.base_url.as_deref()).or_else(|| {
            existing
                .as_ref()
                .and_then(|item| normalize_optional(item.base_url.as_deref()))
        }),
        api_key,
        enabled: draft
            .enabled
            .or_else(|| existing.as_ref().map(|item| item.enabled))
            .unwrap_or(true),
        supports_images: draft
            .supports_images
            .or_else(|| existing.as_ref().map(|item| item.supports_images))
            .unwrap_or(false),
        supports_reasoning: draft
            .supports_reasoning
            .or_else(|| existing.as_ref().map(|item| item.supports_reasoning))
            .unwrap_or(false),
        supports_responses: draft
            .supports_responses
            .or_else(|| existing.as_ref().map(|item| item.supports_responses))
            .unwrap_or(true),
        thinking_level: optional_text_update(
            draft.thinking_level.as_deref(),
            existing
                .as_ref()
                .and_then(|item| item.thinking_level.as_deref()),
        ),
        task_usage_scenario: optional_text_update(
            draft.task_usage_scenario.as_deref(),
            existing
                .as_ref()
                .and_then(|item| item.task_usage_scenario.as_deref()),
        ),
        task_thinking_level: optional_text_update(
            draft.task_thinking_level.as_deref(),
            existing
                .as_ref()
                .and_then(|item| item.task_thinking_level.as_deref()),
        ),
        temperature: if draft.clear_temperature.unwrap_or(false) {
            None
        } else {
            draft
                .temperature
                .or_else(|| existing.as_ref().and_then(|item| item.temperature))
        },
        max_output_tokens: if draft.clear_max_output_tokens.unwrap_or(false) {
            None
        } else {
            draft
                .max_output_tokens
                .or_else(|| existing.as_ref().and_then(|item| item.max_output_tokens))
        },
        created_at: existing
            .as_ref()
            .map(|item| item.created_at.clone())
            .unwrap_or_else(|| now.clone()),
        updated_at: now,
    };
    if let Some(index) = existing_index {
        state.model_configs.configs[index] = record.clone();
    } else {
        state.model_configs.configs.push(record.clone());
    }
    Ok(record)
}

pub(crate) async fn sync_local_model_config(
    http_client: &reqwest::Client,
    state: &mut LocalState,
    local_model_config_id: &str,
) -> Result<LocalModelConfigRecord> {
    let auth = state
        .auth
        .clone()
        .ok_or_else(|| anyhow!("Local Connector must be logged in before syncing model configs"))?;
    let owner_user_id = owner_user_id_from_auth(&auth)?;
    let index = state
        .model_configs
        .configs
        .iter()
        .position(|item| item.id == local_model_config_id)
        .ok_or_else(|| anyhow!("local model config not found: {local_model_config_id}"))?;
    let current = state.model_configs.configs[index].clone();
    let server_model_config_id = current
        .server_model_config_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let payload = json!({
        "id": server_model_config_id.unwrap_or(current.id.as_str()),
        "owner_user_id": owner_user_id,
        "name": current.name,
        "provider": current.provider,
        "model": current.model,
        "thinking_level": current.thinking_level,
        "task_usage_scenario": current.task_usage_scenario.clone().unwrap_or_default(),
        "task_thinking_level": current.task_thinking_level.clone().unwrap_or_default(),
        "has_api_key": current.api_key.as_deref().map(str::trim).is_some_and(|value| !value.is_empty()),
        "enabled": current.enabled,
        "supports_images": current.supports_images,
        "supports_reasoning": current.supports_reasoning,
        "supports_responses": current.supports_responses,
    });
    let (method, path) = if let Some(server_model_config_id) = server_model_config_id {
        (
            Method::PATCH,
            format!(
                "/api/model-configs/{}",
                urlencoding::encode(server_model_config_id)
            ),
        )
    } else {
        (Method::POST, "/api/model-configs".to_string())
    };
    let saved = request_user_service_json::<Value, Value>(
        http_client,
        &auth,
        method.clone(),
        path.as_str(),
        Some(&payload),
    )
    .await;
    let saved = match saved {
        Ok(saved) => saved,
        Err(err) if method == Method::PATCH && is_user_service_not_found(&err) => {
            request_user_service_json::<Value, Value>(
                http_client,
                &auth,
                Method::POST,
                "/api/model-configs",
                Some(&payload),
            )
            .await?
        }
        Err(err) => return Err(err),
    };
    let server_model_config_id = saved
        .get("id")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow!("user_service model config response missing id"))?
        .to_string();
    state.model_configs.configs[index].server_model_config_id = Some(server_model_config_id);
    state.model_configs.configs[index].updated_at = local_now_rfc3339();
    Ok(state.model_configs.configs[index].clone())
}

pub(crate) async fn reconcile_local_model_configs(
    http_client: &reqwest::Client,
    state: &mut LocalState,
) -> Result<usize> {
    if state.model_configs.configs.is_empty() {
        return Ok(0);
    }
    let auth = state.auth.clone().ok_or_else(|| {
        anyhow!("Local Connector must be logged in before reconciling model configs")
    })?;
    let remote = request_user_service_json::<(), Vec<Value>>(
        http_client,
        &auth,
        Method::GET,
        "/api/model-configs",
        None,
    )
    .await?;
    let remote_ids = remote
        .iter()
        .filter_map(|item| item.get("id").and_then(Value::as_str))
        .map(str::trim)
        .filter(|id| !id.is_empty())
        .map(ToOwned::to_owned)
        .collect::<HashSet<_>>();
    let missing_local_ids = state
        .model_configs
        .configs
        .iter()
        .filter(|item| {
            let server_id = item
                .server_model_config_id
                .as_deref()
                .map(str::trim)
                .filter(|id| !id.is_empty())
                .unwrap_or(item.id.as_str());
            !remote_ids.contains(server_id)
        })
        .map(|item| item.id.clone())
        .collect::<Vec<_>>();

    for local_id in &missing_local_ids {
        sync_local_model_config(http_client, state, local_id.as_str()).await?;
    }
    Ok(missing_local_ids.len())
}

pub(crate) async fn delete_local_model_config(
    http_client: &reqwest::Client,
    state: &mut LocalState,
    local_model_config_id: &str,
) -> Result<()> {
    let Some(index) = state
        .model_configs
        .configs
        .iter()
        .position(|item| item.id == local_model_config_id)
    else {
        return Ok(());
    };
    let removed = state.model_configs.configs.remove(index);
    state
        .model_configs
        .settings
        .clear_model_id(local_model_config_id);
    if let Some(server_id) = removed.server_model_config_id.as_deref() {
        if let Some(auth) = state.auth.clone() {
            let path = format!("/api/model-configs/{}", urlencoding::encode(server_id));
            let _ = request_user_service_json::<(), Value>(
                http_client,
                &auth,
                Method::DELETE,
                path.as_str(),
                None,
            )
            .await;
        }
    }
    Ok(())
}

pub(crate) fn save_local_model_settings(
    state: &mut LocalState,
    mut settings: LocalModelSettings,
) -> Result<LocalModelSettings> {
    settings.memory_summary_model_config_id =
        normalize_optional(settings.memory_summary_model_config_id.as_deref());
    settings.memory_summary_thinking_level =
        normalize_optional(settings.memory_summary_thinking_level.as_deref());
    settings.project_management_agent_model_config_id =
        normalize_optional(settings.project_management_agent_model_config_id.as_deref());
    settings.project_management_agent_thinking_level =
        normalize_optional(settings.project_management_agent_thinking_level.as_deref());
    settings.environment_initialization_model_config_id = normalize_optional(
        settings
            .environment_initialization_model_config_id
            .as_deref(),
    );
    settings.environment_initialization_thinking_level = normalize_optional(
        settings
            .environment_initialization_thinking_level
            .as_deref(),
    );
    settings.command_approval_model_config_id =
        normalize_optional(settings.command_approval_model_config_id.as_deref());
    settings.command_approval_thinking_level =
        normalize_optional(settings.command_approval_thinking_level.as_deref());
    settings.updated_at = Some(local_now_rfc3339());
    state.model_configs.settings = settings.clone();
    Ok(settings)
}

pub(crate) async fn sync_local_model_settings(
    http_client: &reqwest::Client,
    state: &LocalState,
) -> Result<LocalModelSettings> {
    let auth = state.auth.clone().ok_or_else(|| {
        anyhow!("Local Connector must be logged in before syncing model settings")
    })?;
    let owner_user_id = owner_user_id_from_auth(&auth)?;
    let local = &state.model_configs.settings;
    let payload = json!({
        "user_id": owner_user_id,
        "memory_summary_model_config_id": local
            .memory_summary_model_config_id
            .as_deref()
            .and_then(|id| server_model_id_for_local(state, id)),
        "memory_summary_thinking_level": local.memory_summary_thinking_level,
        "project_management_agent_model_config_id": local
            .project_management_agent_model_config_id
            .as_deref()
            .and_then(|id| server_model_id_for_local(state, id)),
        "project_management_agent_thinking_level": local.project_management_agent_thinking_level,
    });
    let _ = request_user_service_json::<Value, Value>(
        http_client,
        &auth,
        Method::PUT,
        "/api/model-configs/settings",
        Some(&payload),
    )
    .await?;
    Ok(local.clone())
}

pub(crate) fn resolve_local_model_runtime(
    state: &LocalState,
    owner_user_id: &str,
    model_config_id: &str,
) -> Result<LocalModelRuntimeResponse> {
    let auth = state
        .auth
        .as_ref()
        .ok_or_else(|| anyhow!("Local Connector client is not logged in"))?;
    let paired_owner = owner_user_id_from_auth(auth)?;
    if paired_owner != owner_user_id.trim() {
        return Err(anyhow!(
            "Local Connector is paired to a different user; cannot resolve this model runtime"
        ));
    }
    let model_config_id = model_config_id.trim();
    if model_config_id.is_empty() {
        return Err(anyhow!("model_config_id is required"));
    }
    let record = state
        .model_configs
        .configs
        .iter()
        .find(|item| {
            item.server_model_config_id.as_deref() == Some(model_config_id)
                || item.id.as_str() == model_config_id
        })
        .ok_or_else(|| {
            anyhow!("model config is not mapped in this Local Connector: {model_config_id}")
        })?;
    if !record.enabled {
        return Err(anyhow!("model config is disabled: {model_config_id}"));
    }
    let api_key = record
        .api_key
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow!("model config has no local API key: {model_config_id}"))?
        .to_string();
    let base_url = record
        .base_url
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| default_base_url_for_provider(record.provider.as_str()));
    Ok(LocalModelRuntimeResponse {
        id: record
            .server_model_config_id
            .clone()
            .unwrap_or_else(|| record.id.clone()),
        local_model_config_id: record.id.clone(),
        provider: runtime_provider_for_model(record.provider.as_str(), base_url.as_str()),
        base_url,
        api_key,
        model: record.model.clone(),
        thinking_level: record.thinking_level.clone(),
        supports_images: record.supports_images,
        supports_reasoning: record.supports_reasoning,
        supports_responses: record.supports_responses,
        temperature: record.temperature,
        max_output_tokens: record.max_output_tokens,
    })
}

pub(crate) async fn handle_model_runtime_request(value: Value, _state: &LocalState) -> Value {
    let request = match serde_json::from_value::<RelayRequest>(value) {
        Ok(request) => request,
        Err(err) => {
            return relay_error_response("model_runtime_response", "", 400, err.to_string());
        }
    };
    relay_error_response(
        "model_runtime_response",
        request.request_id.as_str(),
        403,
        "Local model credentials are device-only; remote model runtime requests are disabled"
            .to_string(),
    )
}

fn server_model_id_for_local(state: &LocalState, local_model_config_id: &str) -> Option<String> {
    state
        .model_configs
        .configs
        .iter()
        .find(|item| item.id == local_model_config_id)
        .and_then(|item| item.server_model_config_id.clone())
}

fn owner_user_id_from_auth(auth: &AuthState) -> Result<String> {
    auth.user
        .as_ref()
        .map(|user| user.id.trim().to_string())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow!("current user is unavailable"))
}

async fn request_user_service_json<TBody, TResp>(
    http_client: &reqwest::Client,
    auth: &AuthState,
    method: Method,
    path: &str,
    body: Option<&TBody>,
) -> Result<TResp>
where
    TBody: serde::Serialize + ?Sized,
    TResp: DeserializeOwned,
{
    let endpoint = api_url(auth.user_service_base_url.as_str(), path);
    let mut request = http_client
        .request(method, endpoint.as_str())
        .bearer_auth(auth.access_token.trim());
    if let Some(body) = body {
        request = request.json(body);
    }
    let response = request
        .send()
        .await
        .with_context(|| format!("request user_service {endpoint} failed"))?;
    let status = response.status();
    let text = response.text().await.unwrap_or_default();
    if !status.is_success() {
        return Err(anyhow!(
            "user_service request failed: {} {}",
            status.as_u16(),
            extract_error_message(text.as_str())
        ));
    }
    if text.trim().is_empty() {
        return serde_json::from_value(Value::Null).context("decode empty user_service response");
    }
    serde_json::from_str::<TResp>(text.as_str())
        .with_context(|| format!("decode user_service response failed: {text}"))
}

fn extract_error_message(body: &str) -> String {
    serde_json::from_str::<Value>(body)
        .ok()
        .and_then(|value| {
            value
                .get("error")
                .and_then(Value::as_str)
                .or_else(|| value.get("message").and_then(Value::as_str))
                .map(ToOwned::to_owned)
        })
        .unwrap_or_else(|| body.trim().to_string())
}

fn is_user_service_not_found(error: &anyhow::Error) -> bool {
    error
        .to_string()
        .starts_with("user_service request failed: 404 ")
}

fn required_text(value: Option<String>, field: &str) -> Result<String> {
    normalize_optional(value.as_deref())
        .ok_or_else(|| anyhow!("{field} is required and cannot be empty"))
}

fn optional_text_update(draft: Option<&str>, existing: Option<&str>) -> Option<String> {
    match draft {
        Some(value) => normalize_optional(Some(value)),
        None => existing.and_then(|value| normalize_optional(Some(value))),
    }
}

impl LocalModelSettings {
    fn clear_model_id(&mut self, local_model_config_id: &str) {
        if self.memory_summary_model_config_id.as_deref() == Some(local_model_config_id) {
            self.memory_summary_model_config_id = None;
            self.memory_summary_thinking_level = None;
        }
        if self.project_management_agent_model_config_id.as_deref() == Some(local_model_config_id) {
            self.project_management_agent_model_config_id = None;
            self.project_management_agent_thinking_level = None;
        }
        if self.environment_initialization_model_config_id.as_deref() == Some(local_model_config_id)
        {
            self.environment_initialization_model_config_id = None;
            self.environment_initialization_thinking_level = None;
        }
        if self.command_approval_model_config_id.as_deref() == Some(local_model_config_id) {
            self.command_approval_model_config_id = None;
            self.command_approval_thinking_level = None;
        }
        self.updated_at = Some(local_now_rfc3339());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deleting_environment_model_clears_environment_defaults() {
        let mut settings = LocalModelSettings {
            environment_initialization_model_config_id: Some("environment-model".to_string()),
            environment_initialization_thinking_level: Some("high".to_string()),
            ..Default::default()
        };

        settings.clear_model_id("environment-model");

        assert!(settings
            .environment_initialization_model_config_id
            .is_none());
        assert!(settings.environment_initialization_thinking_level.is_none());
    }

    #[test]
    fn optional_text_update_can_clear_existing_value() {
        assert_eq!(optional_text_update(Some(""), Some("task planning")), None);
        assert_eq!(
            optional_text_update(None, Some("task planning")).as_deref(),
            Some("task planning")
        );
    }

    #[tokio::test]
    async fn remote_model_runtime_requests_never_return_device_credentials() {
        let response = handle_model_runtime_request(
            json!({
                "type": "model_runtime_request",
                "request_id": "request-1",
                "owner_user_id": "user-1",
                "device_id": "device-1",
                "workspace_id": "",
                "method": "GET",
                "path": "/model-runtime/model-1",
                "headers": {},
                "body": {"model_config_id": "model-1"}
            }),
            &LocalState::default(),
        )
        .await;
        assert_eq!(response.get("status").and_then(Value::as_u64), Some(403));
        assert_eq!(
            response
                .pointer("/body/error")
                .and_then(Value::as_str),
            Some(
                "Local model credentials are device-only; remote model runtime requests are disabled"
            )
        );
    }
}
