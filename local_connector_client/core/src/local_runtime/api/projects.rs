// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::extract::{Path, State};
use axum::Json;
use serde::Deserialize;

use crate::local_runtime::storage::{LocalProjectRecord, UpsertLocalProjectInput};
use crate::workspace::paths::resolve_workspace_dir;
use crate::LocalRuntime;

use super::context::owner_context;
use super::error::LocalRuntimeApiError;

#[derive(Debug, Deserialize)]
pub(super) struct UpsertLocalProjectRequest {
    project_name: String,
    workspace_id: String,
    root_relative_path: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(super) struct CreateLocalProjectRequest {
    project_name: String,
    workspace_id: String,
    root_relative_path: Option<String>,
}

pub(super) async fn list_projects(
    State(runtime): State<LocalRuntime>,
) -> Result<Json<Vec<LocalProjectRecord>>, LocalRuntimeApiError> {
    let owner = owner_context(&runtime).await?;
    runtime
        .local_database()?
        .list_projects(owner.owner_user_id.as_str())
        .await
        .map(Json)
        .map_err(LocalRuntimeApiError::from)
}

pub(super) async fn create_project(
    State(runtime): State<LocalRuntime>,
    Json(request): Json<CreateLocalProjectRequest>,
) -> Result<Json<LocalProjectRecord>, LocalRuntimeApiError> {
    upsert(
        &runtime,
        uuid::Uuid::new_v4().to_string(),
        request.project_name,
        request.workspace_id,
        request.root_relative_path,
    )
    .await
    .map(Json)
}

pub(super) async fn get_project(
    Path(project_id): Path<String>,
    State(runtime): State<LocalRuntime>,
) -> Result<Json<LocalProjectRecord>, LocalRuntimeApiError> {
    let owner = owner_context(&runtime).await?;
    let project_id = required(project_id, "project_id")?;
    runtime
        .local_database()?
        .get_project(project_id.as_str(), owner.owner_user_id.as_str())
        .await
        .map_err(LocalRuntimeApiError::from)?
        .map(Json)
        .ok_or_else(|| {
            LocalRuntimeApiError::not_found(
                "local_runtime_project_not_found",
                "Local project was not found",
            )
        })
}

pub(super) async fn upsert_project(
    Path(project_id): Path<String>,
    State(runtime): State<LocalRuntime>,
    Json(request): Json<UpsertLocalProjectRequest>,
) -> Result<Json<LocalProjectRecord>, LocalRuntimeApiError> {
    upsert(
        &runtime,
        project_id,
        request.project_name,
        request.workspace_id,
        request.root_relative_path,
    )
    .await
    .map(Json)
}

pub(super) async fn delete_project(
    Path(project_id): Path<String>,
    State(runtime): State<LocalRuntime>,
) -> Result<Json<serde_json::Value>, LocalRuntimeApiError> {
    let owner = owner_context(&runtime).await?;
    let project_id = required(project_id, "project_id")?;
    let deleted = runtime
        .local_database()?
        .delete_project(project_id.as_str(), owner.owner_user_id.as_str())
        .await
        .map_err(LocalRuntimeApiError::from)?;
    if !deleted {
        return Err(LocalRuntimeApiError::not_found(
            "local_runtime_project_not_found",
            "Local project was not found",
        ));
    }
    Ok(Json(serde_json::json!({ "success": true })))
}

async fn upsert(
    runtime: &LocalRuntime,
    project_id: String,
    project_name: String,
    workspace_id: String,
    root_relative_path: Option<String>,
) -> Result<LocalProjectRecord, LocalRuntimeApiError> {
    let owner = owner_context(runtime).await?;
    let project_id = required(project_id, "project_id")?;
    let project_name = required(project_name, "project_name")?;
    let workspace_id = required(workspace_id, "workspace_id")?;
    let root_relative_path = normalize_optional(root_relative_path);
    {
        let state = runtime.state.read().await;
        let workspace = state
            .workspace_by_id(workspace_id.as_str())
            .ok_or_else(|| {
                LocalRuntimeApiError::bad_request(
                    "local_runtime_workspace_not_found",
                    "The selected workspace is not registered on this device",
                )
            })?;
        resolve_workspace_dir(workspace, root_relative_path.as_deref().unwrap_or(".")).map_err(
            |error| {
                LocalRuntimeApiError::bad_request(
                    "local_runtime_project_root_invalid",
                    error.to_string(),
                )
            },
        )?;
    }

    runtime
        .local_database()?
        .upsert_project(UpsertLocalProjectInput {
            project_id,
            owner_user_id: owner.owner_user_id,
            device_id: owner.device_id,
            workspace_id,
            project_name,
            root_relative_path,
        })
        .await
        .map_err(LocalRuntimeApiError::from)
}

fn required(value: String, field: &'static str) -> Result<String, LocalRuntimeApiError> {
    let value = value.trim().to_string();
    if value.is_empty() {
        return Err(LocalRuntimeApiError::bad_request(
            "local_runtime_invalid_request",
            format!("{field} is required"),
        ));
    }
    Ok(value)
}

fn normalize_optional(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().trim_matches('/').to_string())
        .filter(|value| !value.is_empty())
}
