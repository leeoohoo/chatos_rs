// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

pub(in crate::api) async fn start_task_run(
    Path(id): Path<String>,
    State(state): State<AppState>,
    Extension(current_user): Extension<CurrentUser>,
    Json(input): Json<StartTaskRunRequest>,
) -> Result<(StatusCode, Json<TaskRunRecord>), ApiError> {
    get_task_for_user(&state, &id, &current_user)
        .await?
        .ok_or_else(|| ApiError::not_found(format!("任务不存在: {id}")))?;
    let run = state
        .run_service
        .start_run_for_user(&id, input, &current_user)
        .await
        .map_err(ApiError::bad_request)?;
    Ok((StatusCode::CREATED, Json(run)))
}

pub(in crate::api) async fn get_run(
    Path(id): Path<String>,
    State(state): State<AppState>,
    Extension(current_user): Extension<CurrentUser>,
) -> Result<Json<TaskRunRecord>, ApiError> {
    let run = state
        .run_service
        .get_run(&id)
        .await
        .map_err(ApiError::bad_request)?
        .ok_or_else(|| ApiError::not_found(format!("运行记录不存在: {id}")))?;
    ensure_run_access(&state, &run, &current_user).await?;
    Ok(Json(run))
}

pub(in crate::api) async fn list_run_events(
    Path(id): Path<String>,
    State(state): State<AppState>,
    Extension(current_user): Extension<CurrentUser>,
) -> Result<Json<Vec<TaskRunEventRecord>>, ApiError> {
    let run = state
        .run_service
        .get_run(&id)
        .await
        .map_err(ApiError::bad_request)?
        .ok_or_else(|| ApiError::not_found(format!("运行记录不存在: {id}")))?;
    ensure_run_access(&state, &run, &current_user).await?;
    let events = state
        .run_service
        .list_run_events(&id)
        .await
        .map_err(ApiError::bad_request)?;
    Ok(Json(events))
}

pub(in crate::api) async fn cancel_run(
    Path(id): Path<String>,
    State(state): State<AppState>,
    Extension(current_user): Extension<CurrentUser>,
) -> Result<Json<TaskRunRecord>, ApiError> {
    let existing = state
        .run_service
        .get_run(&id)
        .await
        .map_err(ApiError::bad_request)?
        .ok_or_else(|| ApiError::not_found(format!("运行记录不存在: {id}")))?;
    ensure_run_access(&state, &existing, &current_user).await?;
    let run = state
        .run_service
        .cancel_run(&id)
        .await
        .map_err(ApiError::bad_request)?
        .ok_or_else(|| ApiError::not_found(format!("运行记录不存在: {id}")))?;
    Ok(Json(run))
}

pub(in crate::api) async fn retry_run(
    Path(id): Path<String>,
    State(state): State<AppState>,
    Extension(current_user): Extension<CurrentUser>,
) -> Result<(StatusCode, Json<TaskRunRecord>), ApiError> {
    let existing = state
        .run_service
        .get_run(&id)
        .await
        .map_err(ApiError::bad_request)?
        .ok_or_else(|| ApiError::not_found(format!("运行记录不存在: {id}")))?;
    ensure_run_access(&state, &existing, &current_user).await?;
    let run = state
        .run_service
        .retry_run_for_user(&id, &current_user)
        .await
        .map_err(ApiError::bad_request)?
        .ok_or_else(|| ApiError::not_found(format!("运行记录不存在: {id}")))?;
    Ok((StatusCode::CREATED, Json(run)))
}
