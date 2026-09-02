// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::extract::{Path, State};
use axum::http::HeaderMap;
use axum::Json;
use serde::Serialize;
use std::collections::HashMap;

use crate::models::DEFAULT_MODEL_REQUEST_MAX_RETRIES;
use crate::state::AppState;
use crate::store::now_rfc3339;
use chatos_plugin_management_sdk::normalize_agent_prompt_vendor;

use super::internal_auth::{
    record_user_service_internal_resource_access, require_task_runner_internal_request,
    require_user_model_internal_request, UserServiceInternalResourceAudit,
    MODEL_RUNTIME_READ_SCOPE, MODEL_SETTINGS_READ_SCOPE, TASK_MODEL_CATALOG_READ_SCOPE,
};
use super::models::{is_supported_provider, model_config_has_backing_provider};
use super::{bad_request, forbidden, internal_error, not_found, ApiResult};

#[derive(Debug, Serialize)]
pub struct InternalModelRuntimeConfigResponse {
    pub id: String,
    pub owner_user_id: String,
    pub name: String,
    pub provider: String,
    pub prompt_vendor: Option<String>,
    pub base_url: String,
    pub api_key: String,
    pub model: String,
    pub thinking_level: Option<String>,
    pub temperature: Option<f64>,
    pub max_output_tokens: Option<i64>,
    pub supports_images: bool,
    pub supports_reasoning: bool,
    pub supports_responses: bool,
}

#[derive(Debug, Serialize)]
pub struct InternalUserModelSettingsResponse {
    pub user_id: String,
    pub model_request_max_retries: i64,
    pub memory_summary_model_config_id: Option<String>,
    pub memory_summary_thinking_level: Option<String>,
    pub project_management_agent_model_config_id: Option<String>,
    pub project_management_agent_thinking_level: Option<String>,
    pub updated_at: String,
}

#[derive(Debug, Serialize)]
pub struct InternalTaskModelConfigResponse {
    pub id: String,
    pub owner_user_id: Option<String>,
    pub owner_username: Option<String>,
    pub owner_display_name: Option<String>,
    pub name: String,
    pub provider: String,
    pub prompt_vendor: Option<String>,
    pub base_url: String,
    pub api_key: String,
    pub model: String,
    pub usage_scenario: Option<String>,
    pub temperature: Option<f64>,
    pub max_output_tokens: Option<i64>,
    pub model_request_max_retries: usize,
    pub thinking_level: Option<String>,
    pub supports_images: bool,
    pub supports_reasoning: bool,
    pub supports_responses: bool,
    pub instructions: Option<String>,
    pub request_cwd: Option<String>,
    pub include_prompt_cache_retention: bool,
    pub request_body_limit_bytes: Option<usize>,
    pub enabled: bool,
    pub created_at: String,
    pub updated_at: String,
}

pub async fn list_task_model_configs(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> ApiResult<Vec<InternalTaskModelConfigResponse>> {
    let identity = require_task_runner_internal_request(
        &state.config,
        &headers,
        TASK_MODEL_CATALOG_READ_SCOPE,
    )?;
    let result = load_task_model_configs(&state, None).await.map(Json);
    record_user_service_internal_resource_access(
        &identity,
        UserServiceInternalResourceAudit {
            represented_user_id: None,
            project_id: None,
            resource_type: "task_model_catalog",
            resource_id: "all",
            resource_name: None,
            action: "read",
            outcome: if result.is_ok() {
                "succeeded"
            } else {
                "failed"
            },
        },
    );
    result
}

pub async fn get_task_model_config(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(model_config_id): Path<String>,
) -> ApiResult<InternalTaskModelConfigResponse> {
    let identity = require_task_runner_internal_request(
        &state.config,
        &headers,
        TASK_MODEL_CATALOG_READ_SCOPE,
    )?;
    let model_config_id = model_config_id.trim().to_string();
    let result = async {
        if model_config_id.is_empty() {
            return Err(bad_request("model_config_id is required"));
        }
        load_task_model_configs(&state, Some(model_config_id.as_str()))
            .await?
            .into_iter()
            .next()
            .ok_or_else(|| not_found("model config not found"))
            .map(Json)
    }
    .await;
    record_user_service_internal_resource_access(
        &identity,
        UserServiceInternalResourceAudit {
            represented_user_id: None,
            project_id: None,
            resource_type: "task_model_config",
            resource_id: if model_config_id.is_empty() {
                "unknown"
            } else {
                model_config_id.as_str()
            },
            resource_name: None,
            action: "read",
            outcome: if result.is_ok() {
                "succeeded"
            } else {
                "failed"
            },
        },
    );
    result
}

async fn load_task_model_configs(
    state: &AppState,
    only_id: Option<&str>,
) -> Result<Vec<InternalTaskModelConfigResponse>, (axum::http::StatusCode, Json<serde_json::Value>)>
{
    let configs = match only_id {
        Some(id) => state
            .store
            .find_user_model_config_by_id(id)
            .await
            .map_err(internal_error)?
            .into_iter()
            .collect(),
        None => state
            .store
            .list_user_model_configs(None)
            .await
            .map_err(internal_error)?,
    };
    let providers = state
        .store
        .list_user_model_providers(None)
        .await
        .map_err(internal_error)?;
    let mut retries_by_user = HashMap::new();
    let mut out = Vec::new();
    for config in configs {
        if config.model.trim().is_empty()
            || !is_supported_provider(config.provider.as_str())
            || !model_config_has_backing_provider(&config, providers.as_slice())
        {
            continue;
        }
        let Some(api_key) = config
            .api_key
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
        else {
            continue;
        };
        let Some(base_url) = config
            .base_url
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
        else {
            continue;
        };
        let retries = if let Some(retries) = retries_by_user.get(&config.owner_user_id) {
            *retries
        } else {
            let retries = state
                .store
                .get_user_model_settings(config.owner_user_id.as_str())
                .await
                .map_err(internal_error)?
                .map(|settings| settings.model_request_max_retries)
                .unwrap_or(DEFAULT_MODEL_REQUEST_MAX_RETRIES);
            let retries = usize::try_from(retries)
                .unwrap_or(usize::try_from(DEFAULT_MODEL_REQUEST_MAX_RETRIES).unwrap_or(5));
            retries_by_user.insert(config.owner_user_id.clone(), retries);
            retries
        };
        let enabled = config.enabled_for_tasks();
        out.push(InternalTaskModelConfigResponse {
            id: config.id,
            owner_user_id: Some(config.owner_user_id),
            owner_username: None,
            owner_display_name: None,
            name: config.name,
            provider: config.provider,
            prompt_vendor: config.prompt_vendor,
            base_url,
            api_key,
            model: config.model,
            usage_scenario: config.task_usage_scenario,
            temperature: config.temperature,
            max_output_tokens: config.max_output_tokens,
            model_request_max_retries: retries,
            thinking_level: config.task_thinking_level,
            supports_images: config.supports_images,
            supports_reasoning: config.supports_reasoning,
            supports_responses: config.supports_responses,
            instructions: None,
            request_cwd: None,
            include_prompt_cache_retention: false,
            request_body_limit_bytes: None,
            enabled,
            created_at: config.created_at,
            updated_at: config.updated_at,
        });
    }
    Ok(out)
}

pub async fn get_user_model_settings(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(user_id): Path<String>,
) -> ApiResult<InternalUserModelSettingsResponse> {
    let identity =
        require_user_model_internal_request(&state.config, &headers, MODEL_SETTINGS_READ_SCOPE)?;
    let user_id = user_id.trim().to_string();
    let audit_resource_id = if user_id.is_empty() {
        "unknown"
    } else {
        user_id.as_str()
    };
    let result = async {
        if user_id.is_empty() {
            return Err(bad_request("user_id is required"));
        }
        if state
            .store
            .find_user_by_id(user_id.as_str())
            .await
            .map_err(internal_error)?
            .is_none()
        {
            return Err(not_found("user not found"));
        }

        let settings = state
            .store
            .get_user_model_settings(user_id.as_str())
            .await
            .map_err(internal_error)?;
        Ok(Json(match settings {
            Some(settings) => InternalUserModelSettingsResponse {
                user_id: settings.user_id,
                model_request_max_retries: settings.model_request_max_retries,
                memory_summary_model_config_id: settings.memory_summary_model_config_id,
                memory_summary_thinking_level: settings.memory_summary_thinking_level,
                project_management_agent_model_config_id: settings
                    .project_management_agent_model_config_id,
                project_management_agent_thinking_level: settings
                    .project_management_agent_thinking_level,
                updated_at: settings.updated_at,
            },
            None => InternalUserModelSettingsResponse {
                user_id: user_id.clone(),
                model_request_max_retries: DEFAULT_MODEL_REQUEST_MAX_RETRIES,
                memory_summary_model_config_id: None,
                memory_summary_thinking_level: None,
                project_management_agent_model_config_id: None,
                project_management_agent_thinking_level: None,
                updated_at: now_rfc3339(),
            },
        }))
    }
    .await;
    record_user_service_internal_resource_access(
        &identity,
        UserServiceInternalResourceAudit {
            represented_user_id: (!user_id.is_empty()).then_some(user_id.as_str()),
            project_id: None,
            resource_type: "user_model_settings",
            resource_id: audit_resource_id,
            resource_name: None,
            action: "read",
            outcome: if result.is_ok() {
                "succeeded"
            } else {
                "failed"
            },
        },
    );
    result
}

pub async fn get_user_model_runtime_config(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((user_id, model_config_id)): Path<(String, String)>,
) -> ApiResult<InternalModelRuntimeConfigResponse> {
    let identity =
        require_user_model_internal_request(&state.config, &headers, MODEL_RUNTIME_READ_SCOPE)?;
    let user_id = user_id.trim().to_string();
    let model_config_id = model_config_id.trim().to_string();
    let audit_resource_id = if model_config_id.is_empty() {
        "unknown"
    } else {
        model_config_id.as_str()
    };
    let result = async {
        if user_id.is_empty() {
            return Err(bad_request("user_id is required"));
        }
        if model_config_id.is_empty() {
            return Err(bad_request("model_config_id is required"));
        }
        let Some(model_config) = state
            .store
            .find_user_model_config_by_id(model_config_id.as_str())
            .await
            .map_err(internal_error)?
        else {
            return Err(not_found("model config not found"));
        };
        if model_config.owner_user_id != user_id {
            return Err(forbidden("model config does not belong to the target user"));
        }
        if !is_supported_provider(model_config.provider.as_str()) {
            return Err(not_found("model config not found"));
        }
        let providers = state
            .store
            .list_user_model_providers(Some(user_id.as_str()))
            .await
            .map_err(internal_error)?;
        if !model_config_has_backing_provider(&model_config, providers.as_slice()) {
            return Err(bad_request(
                "model config is not backed by an active model provider",
            ));
        }
        if !model_config.enabled {
            return Err(bad_request("model config is disabled"));
        }
        if model_config.model.trim().is_empty() {
            return Err(bad_request("model config requires a concrete model name"));
        }
        let api_key = model_config
            .api_key
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| bad_request("cloud model config requires a stored API key"))?
            .to_string();
        let base_url = model_config
            .base_url
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| bad_request("cloud model config requires a stored base_url"))?
            .to_string();
        let prompt_vendor = model_config.prompt_vendor.clone().or_else(|| {
            normalize_agent_prompt_vendor(None, model_config.provider.as_str())
                .map(|vendor| vendor.as_str().to_string())
        });

        Ok(Json(InternalModelRuntimeConfigResponse {
            id: model_config.id,
            owner_user_id: model_config.owner_user_id,
            name: model_config.name,
            provider: model_config.provider,
            prompt_vendor,
            base_url,
            api_key,
            model: model_config.model,
            thinking_level: model_config.thinking_level,
            temperature: model_config.temperature,
            max_output_tokens: model_config.max_output_tokens,
            supports_images: model_config.supports_images,
            supports_reasoning: model_config.supports_reasoning,
            supports_responses: model_config.supports_responses,
        }))
    }
    .await;
    record_user_service_internal_resource_access(
        &identity,
        UserServiceInternalResourceAudit {
            represented_user_id: (!user_id.is_empty()).then_some(user_id.as_str()),
            project_id: None,
            resource_type: "user_model_runtime_config",
            resource_id: audit_resource_id,
            resource_name: None,
            action: "read",
            outcome: if result.is_ok() {
                "succeeded"
            } else {
                "failed"
            },
        },
    );
    result
}
