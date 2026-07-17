// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::extract::{Path as AxumPath, State};
use axum::Json;
use serde_json::{json, Value};

use crate::api::types::{
    LocalApiError, LocalModelConfigListResponse, PreviewLocalModelCatalogRequest,
    SaveLocalModelConfigRequest, UpdateLocalModelSettingsRequest,
};
use crate::model_configs::{
    delete_local_model_config, list_local_model_configs, preview_local_model_catalog,
    save_local_model_config, save_local_model_settings, sync_local_model_config,
    sync_local_model_settings, LocalModelCatalogResponse, LocalModelConfigPublic,
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

pub(crate) async fn local_preview_model_catalog(
    State(runtime): State<LocalRuntime>,
    Json(req): Json<PreviewLocalModelCatalogRequest>,
) -> Result<Json<LocalModelCatalogResponse>, LocalApiError> {
    let state = runtime.state.read().await.clone();
    let catalog = preview_local_model_catalog(&runtime.http_client, &state, req.draft)
        .await
        .map_err(|err| LocalApiError::bad_gateway(err.to_string()))?;
    Ok(Json(catalog))
}

pub(crate) async fn local_save_model_config(
    State(runtime): State<LocalRuntime>,
    Json(req): Json<SaveLocalModelConfigRequest>,
) -> Result<Json<LocalModelConfigPublic>, LocalApiError> {
    let mut state = runtime.state.write().await;
    let record = save_local_model_config(&mut state, req.draft)?;
    let record = if req.sync.unwrap_or(true) {
        sync_local_model_config(&runtime.http_client, &mut state, record.id.as_str())
            .await
            .map_err(|err| LocalApiError::bad_gateway(err.to_string()))?
    } else {
        record
    };
    state.save(runtime.state_path.as_path())?;
    Ok(Json(record.public_value()))
}

pub(crate) async fn local_update_model_config(
    State(runtime): State<LocalRuntime>,
    AxumPath(id): AxumPath<String>,
    Json(mut req): Json<SaveLocalModelConfigRequest>,
) -> Result<Json<LocalModelConfigPublic>, LocalApiError> {
    req.draft.id = Some(id);
    local_save_model_config(State(runtime), Json(req)).await
}

pub(crate) async fn local_delete_model_config(
    State(runtime): State<LocalRuntime>,
    AxumPath(id): AxumPath<String>,
) -> Result<Json<Value>, LocalApiError> {
    let mut state = runtime.state.write().await;
    delete_local_model_config(&runtime.http_client, &mut state, id.as_str()).await?;
    state.save(runtime.state_path.as_path())?;
    Ok(Json(json!({ "ok": true })))
}

pub(crate) async fn local_sync_model_config(
    State(runtime): State<LocalRuntime>,
    AxumPath(id): AxumPath<String>,
) -> Result<Json<LocalModelConfigPublic>, LocalApiError> {
    let mut state = runtime.state.write().await;
    let record = sync_local_model_config(&runtime.http_client, &mut state, id.as_str())
        .await
        .map_err(|err| LocalApiError::bad_gateway(err.to_string()))?;
    state.save(runtime.state_path.as_path())?;
    Ok(Json(record.public_value()))
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
    let settings = LocalModelSettings {
        memory_summary_model_config_id: req.memory_summary_model_config_id,
        memory_summary_thinking_level: req.memory_summary_thinking_level,
        project_management_agent_model_config_id: req.project_management_agent_model_config_id,
        project_management_agent_thinking_level: req.project_management_agent_thinking_level,
        environment_initialization_model_config_id: req.environment_initialization_model_config_id,
        environment_initialization_thinking_level: req.environment_initialization_thinking_level,
        command_approval_model_config_id: req.command_approval_model_config_id,
        command_approval_thinking_level: req.command_approval_thinking_level,
        updated_at: None,
    };
    let settings = save_local_model_settings(&mut state, settings)?;
    if req.sync.unwrap_or(false) {
        sync_local_model_settings(&runtime.http_client, &state)
            .await
            .map_err(|err| LocalApiError::bad_gateway(err.to_string()))?;
    }
    state.save(runtime.state_path.as_path())?;
    Ok(Json(settings))
}
