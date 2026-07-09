// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::{Extension, Json};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::models::{
    normalize_binding_mode, normalize_optional_text, CurrentUser, LocalConnectorProjectBinding,
};
use crate::state::AppState;

use super::{required_text, validate_device_workspace, ApiError};

#[derive(Debug, Deserialize)]
pub(super) struct ProjectBindingQuery {
    project_id: Option<String>,
    mode: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(super) struct CreateProjectBindingRequest {
    project_id: Option<String>,
    device_id: Option<String>,
    workspace_id: Option<String>,
    mode: Option<String>,
    enabled: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub(super) struct UpdateProjectBindingRequest {
    device_id: Option<String>,
    workspace_id: Option<String>,
    enabled: Option<bool>,
}

pub(super) async fn list_project_bindings(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Query(query): Query<ProjectBindingQuery>,
) -> Result<Json<Vec<LocalConnectorProjectBinding>>, ApiError> {
    let mode = normalize_optional_text(query.mode).map(|value| normalize_binding_mode(Some(value)));
    state
        .store
        .list_project_bindings(
            user.effective_owner_user_id(),
            normalize_optional_text(query.project_id),
            mode,
        )
        .await
        .map(Json)
        .map_err(ApiError::internal)
}

pub(super) async fn create_project_binding(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Json(req): Json<CreateProjectBindingRequest>,
) -> Result<(StatusCode, Json<LocalConnectorProjectBinding>), ApiError> {
    let device_id = required_text(req.device_id, "device_id")?;
    let workspace_id = required_text(req.workspace_id, "workspace_id")?;
    validate_device_workspace(&state, &user, device_id.as_str(), workspace_id.as_str()).await?;
    let binding = LocalConnectorProjectBinding::new(
        user.effective_owner_user_id().to_string(),
        required_text(req.project_id, "project_id")?,
        device_id,
        workspace_id,
        normalize_binding_mode(req.mode),
        req.enabled.unwrap_or(true),
    );
    let saved = state
        .store
        .upsert_project_binding(&binding)
        .await
        .map_err(ApiError::internal)?;
    Ok((StatusCode::CREATED, Json(saved)))
}

pub(super) async fn update_project_binding(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Path(id): Path<String>,
    Json(req): Json<UpdateProjectBindingRequest>,
) -> Result<Json<LocalConnectorProjectBinding>, ApiError> {
    let mut binding = load_owned_project_binding(&state, &user, id.as_str()).await?;
    if let Some(device_id) = normalize_optional_text(req.device_id) {
        binding.device_id = device_id;
    }
    if let Some(workspace_id) = normalize_optional_text(req.workspace_id) {
        binding.workspace_id = workspace_id;
    }
    validate_device_workspace(
        &state,
        &user,
        binding.device_id.as_str(),
        binding.workspace_id.as_str(),
    )
    .await?;
    if let Some(enabled) = req.enabled {
        binding.enabled = enabled;
    }
    state
        .store
        .update_project_binding(&binding)
        .await
        .map_err(ApiError::internal)?;
    load_owned_project_binding(&state, &user, id.as_str())
        .await
        .map(Json)
}

pub(super) async fn delete_project_binding(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Path(id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    load_owned_project_binding(&state, &user, id.as_str()).await?;
    state
        .store
        .delete_project_binding(user.effective_owner_user_id(), id.as_str())
        .await
        .map_err(ApiError::internal)?;
    Ok(Json(json!({ "success": true })))
}

async fn load_owned_project_binding(
    state: &AppState,
    user: &CurrentUser,
    id: &str,
) -> Result<LocalConnectorProjectBinding, ApiError> {
    let binding = state
        .store
        .get_project_binding(id)
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::not_found("Local Connector project binding not found"))?;
    if binding.owner_user_id != user.effective_owner_user_id() {
        return Err(ApiError::forbidden(
            "Local Connector project binding does not belong to current user",
        ));
    }
    Ok(binding)
}
