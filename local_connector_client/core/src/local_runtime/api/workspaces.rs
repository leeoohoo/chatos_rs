// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::extract::{Path, Query, State};
use axum::Json;
use serde::{Deserialize, Serialize};

use crate::workspace::directory_ops::{
    create_workspace_directory, list_workspace_directory, WorkspaceDirectoryListing,
};
use crate::{LocalRuntime, WorkspaceState};

use super::context::owner_context;
use super::error::LocalRuntimeApiError;

#[derive(Debug, Serialize)]
pub(super) struct LocalDeviceResponse {
    id: String,
    display_name: String,
    status: &'static str,
}

#[derive(Debug, Serialize)]
pub(super) struct LocalWorkspaceResponse {
    id: String,
    device_id: String,
    display_name: String,
    local_path_alias: String,
    status: &'static str,
}

#[derive(Debug, Deserialize)]
pub(super) struct LocalDirectoryQuery {
    path: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(super) struct CreateLocalDirectoryRequest {
    path: String,
}

#[derive(Debug, Serialize)]
pub(super) struct CreateLocalDirectoryResponse {
    path: String,
    created: bool,
}

pub(super) async fn list_devices(
    State(runtime): State<LocalRuntime>,
) -> Result<Json<Vec<LocalDeviceResponse>>, LocalRuntimeApiError> {
    let owner = owner_context(&runtime).await?;
    let state = runtime.state.read().await;
    let display_name = state
        .auth
        .as_ref()
        .map(|auth| auth.device_name.trim())
        .filter(|value| !value.is_empty())
        .unwrap_or("Local Connector")
        .to_string();
    Ok(Json(vec![LocalDeviceResponse {
        id: owner.device_id,
        display_name,
        status: "online",
    }]))
}

pub(super) async fn list_workspaces(
    State(runtime): State<LocalRuntime>,
) -> Result<Json<Vec<LocalWorkspaceResponse>>, LocalRuntimeApiError> {
    let owner = owner_context(&runtime).await?;
    let state = runtime.state.read().await;
    Ok(Json(
        state
            .workspaces
            .iter()
            .map(|workspace| LocalWorkspaceResponse {
                id: workspace.id.clone(),
                device_id: owner.device_id.clone(),
                display_name: workspace.alias.clone(),
                local_path_alias: workspace.alias.clone(),
                status: "active",
            })
            .collect(),
    ))
}

pub(super) async fn list_directory(
    Path(workspace_id): Path<String>,
    Query(query): Query<LocalDirectoryQuery>,
    State(runtime): State<LocalRuntime>,
) -> Result<Json<WorkspaceDirectoryListing>, LocalRuntimeApiError> {
    owner_context(&runtime).await?;
    let workspace = workspace(&runtime, workspace_id.as_str()).await?;
    list_workspace_directory(&workspace, query.path.as_deref().unwrap_or("."), false)
        .map(Json)
        .map_err(workspace_error)
}

pub(super) async fn create_directory(
    Path(workspace_id): Path<String>,
    State(runtime): State<LocalRuntime>,
    Json(request): Json<CreateLocalDirectoryRequest>,
) -> Result<Json<CreateLocalDirectoryResponse>, LocalRuntimeApiError> {
    owner_context(&runtime).await?;
    let workspace = workspace(&runtime, workspace_id.as_str()).await?;
    let path =
        create_workspace_directory(&workspace, request.path.as_str()).map_err(workspace_error)?;
    Ok(Json(CreateLocalDirectoryResponse {
        path,
        created: true,
    }))
}

async fn workspace(
    runtime: &LocalRuntime,
    workspace_id: &str,
) -> Result<WorkspaceState, LocalRuntimeApiError> {
    let workspace_id = workspace_id.trim();
    let state = runtime.state.read().await;
    state.workspace_by_id(workspace_id).cloned().ok_or_else(|| {
        LocalRuntimeApiError::bad_request(
            "local_runtime_workspace_not_found",
            "The selected workspace is not registered on this device",
        )
    })
}

fn workspace_error(error: anyhow::Error) -> LocalRuntimeApiError {
    LocalRuntimeApiError::bad_request("local_runtime_workspace_path_invalid", error.to_string())
}
