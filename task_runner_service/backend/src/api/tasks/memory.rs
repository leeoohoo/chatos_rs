// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

pub(in crate::api) async fn get_task_memory_context(
    Path(id): Path<String>,
    State(state): State<AppState>,
    Extension(current_user): Extension<CurrentUser>,
    Query(query): Query<TaskMemoryContextQuery>,
) -> Result<Json<TaskMemoryContextResponse>, ApiError> {
    get_task_for_user(&state, &id, &current_user)
        .await?
        .ok_or_else(|| ApiError::not_found(format!("任务不存在: {id}")))?;
    let response = state
        .task_service
        .get_task_memory_context(&id, query.into_options())
        .await
        .map_err(ApiError::bad_request)?
        .ok_or_else(|| ApiError::not_found(format!("任务不存在: {id}")))?;
    Ok(Json(response))
}

pub(in crate::api) async fn get_task_memory_records(
    Path(id): Path<String>,
    State(state): State<AppState>,
    Extension(current_user): Extension<CurrentUser>,
    Query(query): Query<TaskMemoryRecordsQuery>,
) -> Result<Json<TaskMemoryRecordsResponse>, ApiError> {
    get_task_for_user(&state, &id, &current_user)
        .await?
        .ok_or_else(|| ApiError::not_found(format!("任务不存在: {id}")))?;
    let response = state
        .task_service
        .get_task_memory_records(&id, query.into_options())
        .await
        .map_err(ApiError::bad_request)?
        .ok_or_else(|| ApiError::not_found(format!("任务不存在: {id}")))?;
    Ok(Json(response))
}

pub(in crate::api) async fn summarize_task_memory(
    Path(id): Path<String>,
    State(state): State<AppState>,
    Extension(current_user): Extension<CurrentUser>,
) -> Result<Json<TaskMemorySummaryResponse>, ApiError> {
    get_task_for_user(&state, &id, &current_user)
        .await?
        .ok_or_else(|| ApiError::not_found(format!("任务不存在: {id}")))?;
    let response = state
        .task_service
        .summarize_task_memory(&id)
        .await
        .map_err(ApiError::bad_request)?
        .ok_or_else(|| ApiError::not_found(format!("任务不存在: {id}")))?;
    Ok(Json(response))
}
