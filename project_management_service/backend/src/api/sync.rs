// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::extract::{Path, Query, State};
use axum::http::HeaderMap;
use axum::Json;
use serde::Deserialize;

use super::internal_auth::{
    require_project_internal_request, CHATOS_CALLER, PROJECT_READ_SCOPE, PROJECT_SYNC_SCOPE,
    TASK_RUNNER_CALLER,
};
use super::ApiError;
use crate::models::{
    ImportProjectRequest, ProjectRecord, ProjectRuntimeEnvironmentResponse, ProjectStatus,
    SyncRequirementExecutionStateRequest, SyncRequirementExecutionStateResponse,
    SyncTaskRunnerWorkItemStatusRequest, SyncTaskRunnerWorkItemStatusResponse,
};
use crate::services::execution_sync::{self, ExecutionSyncError};
use crate::services::runtime_environment::{
    default_runtime_environment_for_project, enforce_project_runtime_boundary,
    ensure_runtime_environment_for_project, refresh_environment_variable_values,
};
use crate::state::AppState;

#[derive(Debug, Default, Deserialize)]
pub(in crate::api) struct SyncProjectListQuery {
    status: Option<ProjectStatus>,
}

pub(in crate::api) async fn sync_list_projects(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<SyncProjectListQuery>,
) -> Result<Json<Vec<ProjectRecord>>, ApiError> {
    require_project_internal_request(
        &state.config,
        &headers,
        &[CHATOS_CALLER, TASK_RUNNER_CALLER],
        PROJECT_READ_SCOPE,
    )?;
    state
        .store
        .list_all_projects(query.status)
        .await
        .map(Json)
        .map_err(ApiError::bad_request)
}

pub(in crate::api) async fn sync_import_project(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<ImportProjectRequest>,
) -> Result<Json<ProjectRecord>, ApiError> {
    require_project_internal_request(
        &state.config,
        &headers,
        &[CHATOS_CALLER, TASK_RUNNER_CALLER],
        PROJECT_SYNC_SCOPE,
    )?;
    let sandbox_enabled = input.sandbox_enabled;
    let project = state
        .store
        .import_project(input)
        .await
        .map_err(ApiError::bad_request)?;
    ensure_runtime_environment_for_project(&state.store, &project, sandbox_enabled)
        .await
        .map_err(ApiError::bad_request)?;
    Ok(Json(project))
}

pub(in crate::api) async fn sync_get_project(
    Path(project_id): Path<String>,
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<ProjectRecord>, ApiError> {
    require_project_internal_request(
        &state.config,
        &headers,
        &[CHATOS_CALLER, TASK_RUNNER_CALLER],
        PROJECT_READ_SCOPE,
    )?;
    state
        .store
        .get_project(&project_id)
        .await
        .map_err(ApiError::bad_request)?
        .map(Json)
        .ok_or_else(|| ApiError::not_found(format!("项目不存在: {project_id}")))
}

pub(in crate::api) async fn sync_get_project_runtime_environment(
    Path(project_id): Path<String>,
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<ProjectRuntimeEnvironmentResponse>, ApiError> {
    require_project_internal_request(
        &state.config,
        &headers,
        &[CHATOS_CALLER, TASK_RUNNER_CALLER],
        PROJECT_READ_SCOPE,
    )?;
    let project = state
        .store
        .get_project(&project_id)
        .await
        .map_err(ApiError::bad_request)?
        .ok_or_else(|| ApiError::not_found(format!("项目不存在: {project_id}")))?;
    let mut environment = state
        .store
        .get_project_runtime_environment(&project_id)
        .await
        .map_err(ApiError::bad_request)?
        .unwrap_or_else(|| default_runtime_environment_for_project(&project, None));
    refresh_environment_variable_values(&mut environment);
    let mut images = state
        .store
        .list_project_runtime_environment_images(&project_id)
        .await
        .map_err(ApiError::bad_request)?;
    if enforce_project_runtime_boundary(
        project.execution_plane,
        &mut environment,
        images.as_mut_slice(),
    ) {
        environment = state
            .store
            .upsert_project_runtime_environment(&environment)
            .await
            .map_err(ApiError::bad_request)?;
        images = state
            .store
            .replace_project_runtime_environment_images(&project_id, images.as_slice())
            .await
            .map_err(ApiError::bad_request)?;
    }
    Ok(Json(ProjectRuntimeEnvironmentResponse {
        environment,
        images,
    }))
}

pub(in crate::api) async fn sync_task_runner_work_item_status(
    Path(work_item_id): Path<String>,
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<SyncTaskRunnerWorkItemStatusRequest>,
) -> Result<Json<SyncTaskRunnerWorkItemStatusResponse>, ApiError> {
    require_project_internal_request(
        &state.config,
        &headers,
        &[CHATOS_CALLER, TASK_RUNNER_CALLER],
        PROJECT_SYNC_SCOPE,
    )?;
    execution_sync::sync_task_runner_work_item_status(&state.store, &work_item_id, input)
        .await
        .map(Json)
        .map_err(sync_error_to_api_error)
}

pub(in crate::api) async fn sync_task_runner_task_status(
    Path(task_runner_task_id): Path<String>,
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<SyncTaskRunnerWorkItemStatusRequest>,
) -> Result<Json<SyncTaskRunnerWorkItemStatusResponse>, ApiError> {
    require_project_internal_request(
        &state.config,
        &headers,
        &[CHATOS_CALLER, TASK_RUNNER_CALLER],
        PROJECT_SYNC_SCOPE,
    )?;
    execution_sync::sync_task_runner_task_status(&state.store, &task_runner_task_id, input)
        .await
        .map(Json)
        .map_err(sync_error_to_api_error)
}

pub(in crate::api) async fn sync_requirement_execution_state(
    Path(requirement_id): Path<String>,
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<SyncRequirementExecutionStateRequest>,
) -> Result<Json<SyncRequirementExecutionStateResponse>, ApiError> {
    require_project_internal_request(
        &state.config,
        &headers,
        &[CHATOS_CALLER, TASK_RUNNER_CALLER],
        PROJECT_SYNC_SCOPE,
    )?;
    execution_sync::sync_requirement_execution_state(&state.store, &requirement_id, input)
        .await
        .map(Json)
        .map_err(sync_error_to_api_error)
}

fn sync_error_to_api_error(error: ExecutionSyncError) -> ApiError {
    match error {
        ExecutionSyncError::BadRequest(message) => ApiError::bad_request(message),
        ExecutionSyncError::NotFound(message) => ApiError::not_found(message),
    }
}
