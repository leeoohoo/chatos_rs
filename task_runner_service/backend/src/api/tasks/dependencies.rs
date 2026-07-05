// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

pub(in crate::api) async fn list_task_prerequisites(
    Path(id): Path<String>,
    State(state): State<AppState>,
    Extension(current_user): Extension<CurrentUser>,
) -> Result<Json<Vec<TaskSummaryRecord>>, ApiError> {
    get_task_for_user(&state, &id, &current_user)
        .await?
        .ok_or_else(|| ApiError::not_found(format!("任务不存在: {id}")))?;
    let tasks = state
        .task_service
        .list_task_prerequisites(&id)
        .await
        .map_err(ApiError::bad_request)?;
    Ok(Json(redact_workspace_paths(&state, tasks)?))
}

pub(in crate::api) async fn set_task_prerequisites(
    Path(id): Path<String>,
    State(state): State<AppState>,
    Extension(current_user): Extension<CurrentUser>,
    Json(input): Json<SetTaskPrerequisitesRequest>,
) -> Result<Json<TaskRecord>, ApiError> {
    get_task_for_user(&state, &id, &current_user)
        .await?
        .ok_or_else(|| ApiError::not_found(format!("任务不存在: {id}")))?;
    let task = state
        .task_service
        .set_task_prerequisites(&id, input.prerequisite_task_ids, Some(&current_user))
        .await
        .map_err(ApiError::bad_request)?
        .ok_or_else(|| ApiError::not_found(format!("任务不存在: {id}")))?;
    Ok(Json(redact_workspace_paths(&state, task)?))
}

pub(in crate::api) async fn get_task_dependency_graph(
    Path(id): Path<String>,
    State(state): State<AppState>,
    Extension(current_user): Extension<CurrentUser>,
) -> Result<Json<TaskDependencyGraph>, ApiError> {
    get_task_for_user(&state, &id, &current_user)
        .await?
        .ok_or_else(|| ApiError::not_found(format!("任务不存在: {id}")))?;
    let graph = state
        .task_service
        .get_task_dependency_graph(&id)
        .await
        .map_err(ApiError::bad_request)?
        .ok_or_else(|| ApiError::not_found(format!("任务不存在: {id}")))?;
    Ok(Json(redact_workspace_paths(&state, graph)?))
}
