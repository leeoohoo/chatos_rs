// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::{Extension, Json};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::models::{
    normalize_capabilities, normalize_optional_text, normalize_workspace_status, CurrentUser,
    LocalConnectorWorkspace,
};
use crate::state::AppState;

use super::{load_owned_device, required_text, ApiError};

#[derive(Debug, Deserialize)]
pub(super) struct WorkspaceQuery {
    device_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(super) struct CreateWorkspaceRequest {
    device_id: Option<String>,
    display_name: Option<String>,
    local_path_alias: Option<String>,
    local_path_fingerprint: Option<String>,
    capabilities: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
pub(super) struct UpdateWorkspaceRequest {
    device_id: Option<String>,
    display_name: Option<String>,
    local_path_alias: Option<String>,
    local_path_fingerprint: Option<String>,
    capabilities: Option<Vec<String>>,
    status: Option<String>,
}

pub(super) async fn list_workspaces(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Query(query): Query<WorkspaceQuery>,
) -> Result<Json<Vec<LocalConnectorWorkspace>>, ApiError> {
    state
        .store
        .list_workspaces(user.effective_owner_user_id(), query.device_id)
        .await
        .map(Json)
        .map_err(ApiError::internal)
}

pub(super) async fn create_workspace(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Json(req): Json<CreateWorkspaceRequest>,
) -> Result<(StatusCode, Json<LocalConnectorWorkspace>), ApiError> {
    let device_id = required_text(req.device_id, "device_id")?;
    load_owned_device(&state, &user, device_id.as_str(), true).await?;
    let workspace = LocalConnectorWorkspace::new(
        user.effective_owner_user_id().to_string(),
        device_id,
        required_text(req.display_name, "display_name")?,
        required_text(req.local_path_alias, "local_path_alias")?,
        required_text(req.local_path_fingerprint, "local_path_fingerprint")?,
        normalize_capabilities(req.capabilities.unwrap_or_else(default_capabilities)),
    );
    state
        .store
        .create_workspace(&workspace)
        .await
        .map_err(ApiError::internal)?;
    Ok((StatusCode::CREATED, Json(workspace)))
}

pub(super) async fn update_workspace(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Path(id): Path<String>,
    Json(req): Json<UpdateWorkspaceRequest>,
) -> Result<Json<LocalConnectorWorkspace>, ApiError> {
    let mut workspace = load_owned_workspace(&state, &user, id.as_str()).await?;
    if let Some(device_id) = normalize_optional_text(req.device_id) {
        load_owned_device(&state, &user, device_id.as_str(), true).await?;
        workspace.device_id = device_id;
    }
    if let Some(display_name) = normalize_optional_text(req.display_name) {
        workspace.display_name = display_name;
    }
    if let Some(alias) = normalize_optional_text(req.local_path_alias) {
        workspace.local_path_alias = alias;
    }
    if let Some(fingerprint) = normalize_optional_text(req.local_path_fingerprint) {
        workspace.local_path_fingerprint = fingerprint;
    }
    if let Some(capabilities) = req.capabilities {
        workspace.capabilities = normalize_capabilities(capabilities);
    }
    if let Some(status) = normalize_optional_text(req.status) {
        workspace.status = normalize_workspace_status(Some(status));
    }
    state
        .store
        .update_workspace(&workspace)
        .await
        .map_err(ApiError::internal)?;
    load_owned_workspace(&state, &user, id.as_str())
        .await
        .map(Json)
}

pub(super) async fn delete_workspace(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Path(id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    load_owned_workspace(&state, &user, id.as_str()).await?;
    state
        .store
        .delete_workspace(user.effective_owner_user_id(), id.as_str())
        .await
        .map_err(ApiError::internal)?;
    Ok(Json(json!({ "success": true })))
}

pub(super) async fn load_owned_workspace(
    state: &AppState,
    user: &CurrentUser,
    id: &str,
) -> Result<LocalConnectorWorkspace, ApiError> {
    let workspace = state
        .store
        .get_workspace(id)
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::not_found("Local Connector workspace not found"))?;
    if workspace.owner_user_id != user.effective_owner_user_id() {
        return Err(ApiError::forbidden(
            "Local Connector workspace does not belong to current user",
        ));
    }
    Ok(workspace)
}

fn default_capabilities() -> Vec<String> {
    vec![
        "mcp".to_string(),
        "terminal".to_string(),
        "sandbox".to_string(),
    ]
}
