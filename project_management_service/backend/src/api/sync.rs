// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::extract::{Path, Query, State};
use axum::http::HeaderMap;
use axum::Json;
use serde::Deserialize;

use super::internal_auth::{
    record_project_internal_resource_access, require_project_internal_request,
    ProjectInternalResourceAudit, CHATOS_CALLER, PROJECT_READ_SCOPE, PROJECT_SYNC_SCOPE,
    TASK_RUNNER_CALLER,
};
use super::ApiError;
use crate::models::{
    ImportProjectRequest, ProjectRecord, ProjectStatus, ProjectWorkItemTaskRunnerLinkRecord,
    SyncDeleteExecutionLinksRequest, SyncExecutionLinksQueryRequest,
    SyncRequirementExecutionStateRequest, SyncRequirementExecutionStateResponse,
    SyncTaskRunnerWorkItemStatusRequest, SyncTaskRunnerWorkItemStatusResponse,
};
use crate::services::execution_sync::{self, ExecutionSyncError};
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
    let identity = require_project_internal_request(
        &state.config,
        &headers,
        &[CHATOS_CALLER, TASK_RUNNER_CALLER],
        PROJECT_SYNC_SCOPE,
    )?;
    let resource_id = input.id.clone();
    let represented_user_id = input.owner_user_id.clone();
    let resource_name = input.name.clone();
    let result = async {
        state
            .store
            .import_project(input)
            .await
            .map_err(ApiError::bad_request)
    }
    .await;
    record_project_internal_resource_access(
        &identity,
        ProjectInternalResourceAudit {
            represented_user_id: represented_user_id.as_deref(),
            project_id: Some(resource_id.as_str()),
            resource_type: "project",
            resource_id: resource_id.as_str(),
            resource_name: Some(resource_name.as_str()),
            action: "import",
            outcome: operation_outcome(&result),
        },
    );
    result.map(Json)
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

pub(in crate::api) async fn sync_list_execution_links(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<SyncExecutionLinksQueryRequest>,
) -> Result<Json<Vec<ProjectWorkItemTaskRunnerLinkRecord>>, ApiError> {
    require_project_internal_request(
        &state.config,
        &headers,
        &[CHATOS_CALLER, TASK_RUNNER_CALLER],
        PROJECT_READ_SCOPE,
    )?;
    let mut links = Vec::new();
    for work_item_id in normalized_unique_ids(input.work_item_ids) {
        links.extend(
            state
                .store
                .list_task_runner_links(work_item_id.as_str())
                .await
                .map_err(ApiError::bad_request)?,
        );
    }
    Ok(Json(links))
}

pub(in crate::api) async fn sync_delete_execution_links(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<SyncDeleteExecutionLinksRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    require_project_internal_request(
        &state.config,
        &headers,
        &[CHATOS_CALLER, TASK_RUNNER_CALLER],
        PROJECT_SYNC_SCOPE,
    )?;
    let mut deleted = 0usize;
    for link in input.links {
        let work_item_id = link.work_item_id.trim();
        let link_id = link.link_id.trim();
        if work_item_id.is_empty() || link_id.is_empty() {
            continue;
        }
        if state
            .store
            .delete_task_runner_link(work_item_id, link_id)
            .await
            .map_err(ApiError::bad_request)?
        {
            deleted += 1;
        }
    }
    Ok(Json(serde_json::json!({ "deleted_count": deleted })))
}

pub(in crate::api) async fn sync_task_runner_work_item_status(
    Path(work_item_id): Path<String>,
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<SyncTaskRunnerWorkItemStatusRequest>,
) -> Result<Json<SyncTaskRunnerWorkItemStatusResponse>, ApiError> {
    let identity = require_project_internal_request(
        &state.config,
        &headers,
        &[CHATOS_CALLER, TASK_RUNNER_CALLER],
        PROJECT_SYNC_SCOPE,
    )?;
    let result =
        execution_sync::sync_task_runner_work_item_status(&state.store, &work_item_id, input)
            .await
            .map_err(sync_error_to_api_error);
    let work_item = result.as_ref().ok().map(|response| &response.work_item);
    record_project_internal_resource_access(
        &identity,
        ProjectInternalResourceAudit {
            represented_user_id: work_item.and_then(|item| {
                item.owner_user_id
                    .as_deref()
                    .or(item.creator_user_id.as_deref())
            }),
            project_id: work_item.map(|item| item.project_id.as_str()),
            resource_type: "project_work_item",
            resource_id: work_item_id.as_str(),
            resource_name: work_item.map(|item| item.title.as_str()),
            action: "sync_task_runner_status",
            outcome: operation_outcome(&result),
        },
    );
    result.map(Json)
}

pub(in crate::api) async fn sync_task_runner_task_status(
    Path(task_runner_task_id): Path<String>,
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<SyncTaskRunnerWorkItemStatusRequest>,
) -> Result<Json<SyncTaskRunnerWorkItemStatusResponse>, ApiError> {
    let identity = require_project_internal_request(
        &state.config,
        &headers,
        &[CHATOS_CALLER, TASK_RUNNER_CALLER],
        PROJECT_SYNC_SCOPE,
    )?;
    let result =
        execution_sync::sync_task_runner_task_status(&state.store, &task_runner_task_id, input)
            .await
            .map_err(sync_error_to_api_error);
    let work_item = result.as_ref().ok().map(|response| &response.work_item);
    record_project_internal_resource_access(
        &identity,
        ProjectInternalResourceAudit {
            represented_user_id: work_item.and_then(|item| {
                item.owner_user_id
                    .as_deref()
                    .or(item.creator_user_id.as_deref())
            }),
            project_id: work_item.map(|item| item.project_id.as_str()),
            resource_type: "task_runner_task",
            resource_id: task_runner_task_id.as_str(),
            resource_name: work_item.map(|item| item.title.as_str()),
            action: "sync_project_status",
            outcome: operation_outcome(&result),
        },
    );
    result.map(Json)
}

pub(in crate::api) async fn sync_requirement_execution_state(
    Path(requirement_id): Path<String>,
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<SyncRequirementExecutionStateRequest>,
) -> Result<Json<SyncRequirementExecutionStateResponse>, ApiError> {
    let identity = require_project_internal_request(
        &state.config,
        &headers,
        &[CHATOS_CALLER, TASK_RUNNER_CALLER],
        PROJECT_SYNC_SCOPE,
    )?;
    let result =
        execution_sync::sync_requirement_execution_state(&state.store, &requirement_id, input)
            .await
            .map_err(sync_error_to_api_error);
    let requirement = result.as_ref().ok().map(|response| &response.requirement);
    record_project_internal_resource_access(
        &identity,
        ProjectInternalResourceAudit {
            represented_user_id: requirement.and_then(|item| {
                item.owner_user_id
                    .as_deref()
                    .or(item.creator_user_id.as_deref())
            }),
            project_id: requirement.map(|item| item.project_id.as_str()),
            resource_type: "requirement",
            resource_id: requirement_id.as_str(),
            resource_name: requirement.map(|item| item.title.as_str()),
            action: "sync_execution_state",
            outcome: operation_outcome(&result),
        },
    );
    result.map(Json)
}

fn operation_outcome<T>(result: &Result<T, ApiError>) -> &'static str {
    if result.is_ok() {
        "succeeded"
    } else {
        "failed"
    }
}

fn normalized_unique_ids(values: Vec<String>) -> Vec<String> {
    values
        .into_iter()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn sync_error_to_api_error(error: ExecutionSyncError) -> ApiError {
    match error {
        ExecutionSyncError::BadRequest(message) => ApiError::bad_request(message),
        ExecutionSyncError::NotFound(message) => ApiError::not_found(message),
    }
}
