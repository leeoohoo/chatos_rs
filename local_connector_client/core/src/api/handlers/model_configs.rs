// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::extract::State;
use axum::Json;

use crate::api::types::{
    LocalApiError, LocalModelConfigListResponse, UpdateLocalModelSettingsRequest,
};
use crate::model_configs::{
    list_local_model_configs, reconcile_local_model_configs, save_local_model_settings,
    LocalModelSettings,
};
use crate::LocalRuntime;

pub(crate) async fn local_model_configs(
    State(runtime): State<LocalRuntime>,
) -> Result<Json<LocalModelConfigListResponse>, LocalApiError> {
    let state = runtime.state.read().await;
    Ok(Json(LocalModelConfigListResponse {
        items: list_local_model_configs(&state),
        settings: state.model_configs.settings.clone(),
    }))
}

pub(crate) async fn local_refresh_model_configs(
    State(runtime): State<LocalRuntime>,
) -> Result<Json<LocalModelConfigListResponse>, LocalApiError> {
    let mut state = runtime.state.write().await;
    reconcile_local_model_configs(&runtime.http_client, &mut state)
        .await
        .map_err(|err| LocalApiError::bad_gateway(err.to_string()))?;
    state.save(runtime.state_path.as_path())?;
    Ok(Json(LocalModelConfigListResponse {
        items: list_local_model_configs(&state),
        settings: state.model_configs.settings.clone(),
    }))
}

pub(crate) async fn local_model_settings(
    State(runtime): State<LocalRuntime>,
) -> Result<Json<LocalModelSettings>, LocalApiError> {
    let state = runtime.state.read().await;
    Ok(Json(state.model_configs.settings.clone()))
}

pub(crate) async fn local_update_model_settings(
    State(runtime): State<LocalRuntime>,
    Json(req): Json<UpdateLocalModelSettingsRequest>,
) -> Result<Json<LocalModelSettings>, LocalApiError> {
    let mut state = runtime.state.write().await;
    let model_config_id = req
        .command_approval_model_config_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| LocalApiError::bad_request("command approval model is required"))?;
    let selectable = state.model_configs.configs.iter().any(|item| {
        item.id == model_config_id
            && item.enabled
            && !item.model.trim().is_empty()
            && item
                .api_key
                .as_deref()
                .map(str::trim)
                .is_some_and(|value| !value.is_empty())
    });
    if !selectable {
        return Err(LocalApiError::bad_request(
            "command approval model must be an enabled cloud model with credentials",
        ));
    }
    let model_request_max_retries = req
        .model_request_max_retries
        .unwrap_or(state.model_configs.settings.model_request_max_retries);
    if model_request_max_retries > 10 {
        return Err(LocalApiError::bad_request(
            "model_request_max_retries must be between 0 and 10",
        ));
    }
    let settings = LocalModelSettings {
        model_request_max_retries,
        command_approval_model_config_id: Some(model_config_id.to_string()),
        command_approval_thinking_level: req.command_approval_thinking_level,
        updated_at: None,
    };
    let settings = save_local_model_settings(&mut state, settings)?;
    state.save(runtime.state_path.as_path())?;
    Ok(Json(settings))
}
