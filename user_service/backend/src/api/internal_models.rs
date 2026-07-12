// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::extract::{Path, State};
use axum::http::HeaderMap;
use axum::Json;
use serde::Serialize;

use crate::state::AppState;
use crate::store::now_rfc3339;

use super::{bad_request, forbidden, internal_error, not_found, ApiResult};
use super::internal_auth::{
    require_project_service_internal_request, MODEL_RUNTIME_READ_SCOPE, MODEL_SETTINGS_READ_SCOPE,
};

#[derive(Debug, Serialize)]
pub struct InternalModelRuntimeConfigResponse {
    pub id: String,
    pub owner_user_id: String,
    pub name: String,
    pub provider: String,
    pub base_url: String,
    pub api_key: String,
    pub model: String,
    pub thinking_level: Option<String>,
    pub supports_images: bool,
    pub supports_reasoning: bool,
    pub supports_responses: bool,
}

#[derive(Debug, Serialize)]
pub struct InternalUserModelSettingsResponse {
    pub user_id: String,
    pub memory_summary_model_config_id: Option<String>,
    pub memory_summary_thinking_level: Option<String>,
    pub project_management_agent_model_config_id: Option<String>,
    pub project_management_agent_thinking_level: Option<String>,
    pub updated_at: String,
}

pub async fn get_user_model_settings(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(user_id): Path<String>,
) -> ApiResult<InternalUserModelSettingsResponse> {
    require_project_service_internal_request(
        &state.config,
        &headers,
        MODEL_SETTINGS_READ_SCOPE,
    )?;
    let user_id = user_id.trim();
    if user_id.is_empty() {
        return Err(bad_request("user_id is required"));
    }
    if state
        .store
        .find_user_by_id(user_id)
        .await
        .map_err(internal_error)?
        .is_none()
    {
        return Err(not_found("user not found"));
    }

    let settings = state
        .store
        .get_user_model_settings(user_id)
        .await
        .map_err(internal_error)?;
    Ok(Json(match settings {
        Some(settings) => InternalUserModelSettingsResponse {
            user_id: settings.user_id,
            memory_summary_model_config_id: settings.memory_summary_model_config_id,
            memory_summary_thinking_level: settings.memory_summary_thinking_level,
            project_management_agent_model_config_id: settings
                .project_management_agent_model_config_id,
            project_management_agent_thinking_level: settings
                .project_management_agent_thinking_level,
            updated_at: settings.updated_at,
        },
        None => InternalUserModelSettingsResponse {
            user_id: user_id.to_string(),
            memory_summary_model_config_id: None,
            memory_summary_thinking_level: None,
            project_management_agent_model_config_id: None,
            project_management_agent_thinking_level: None,
            updated_at: now_rfc3339(),
        },
    }))
}

pub async fn get_user_model_runtime_config(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((user_id, model_config_id)): Path<(String, String)>,
) -> ApiResult<InternalModelRuntimeConfigResponse> {
    require_project_service_internal_request(
        &state.config,
        &headers,
        MODEL_RUNTIME_READ_SCOPE,
    )?;
    let user_id = user_id.trim();
    let model_config_id = model_config_id.trim();
    if user_id.is_empty() {
        return Err(bad_request("user_id is required"));
    }
    if model_config_id.is_empty() {
        return Err(bad_request("model_config_id is required"));
    }
    let Some(model_config) = state
        .store
        .find_user_model_config_by_id(model_config_id)
        .await
        .map_err(internal_error)?
    else {
        return Err(not_found("model config not found"));
    };
    if model_config.owner_user_id != user_id {
        return Err(forbidden("model config does not belong to the target user"));
    }
    if !model_config.enabled {
        return Err(bad_request("model config is disabled"));
    }
    if model_config.model.trim().is_empty() {
        return Err(bad_request("model config requires a concrete model name"));
    }

    Ok(Json(InternalModelRuntimeConfigResponse {
        id: model_config.id,
        owner_user_id: model_config.owner_user_id,
        name: model_config.name,
        provider: model_config.provider,
        base_url: String::new(),
        api_key: String::new(),
        model: model_config.model,
        thinking_level: model_config.thinking_level,
        supports_images: model_config.supports_images,
        supports_reasoning: model_config.supports_reasoning,
        supports_responses: model_config.supports_responses,
    }))
}
