// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;
use crate::models::TaskRunWorkspaceExecution;

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
    Ok((
        StatusCode::CREATED,
        Json(redact_workspace_paths(&state, run)?),
    ))
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
    Ok(Json(redact_workspace_paths(&state, run)?))
}

pub(in crate::api) async fn get_run_workspace_integration(
    Path(id): Path<String>,
    State(state): State<AppState>,
    Extension(current_user): Extension<CurrentUser>,
) -> Result<Json<TaskRunWorkspaceExecution>, ApiError> {
    let run = state
        .run_service
        .get_run(&id)
        .await
        .map_err(ApiError::bad_request)?
        .ok_or_else(|| ApiError::not_found(format!("运行记录不存在: {id}")))?;
    ensure_run_access(&state, &run, &current_user).await?;
    let integration = run
        .workspace_execution
        .ok_or_else(|| ApiError::not_found("当前运行没有代码集成上下文"))?;
    Ok(Json(redact_workspace_paths(&state, integration)?))
}

pub(in crate::api) async fn get_run_workspace_changes(
    Path(id): Path<String>,
    State(state): State<AppState>,
    Extension(current_user): Extension<CurrentUser>,
) -> Result<
    Json<crate::services::project_management_api_client::GetRunWorkspaceChangesResponse>,
    ApiError,
> {
    let run = state
        .run_service
        .get_run(&id)
        .await
        .map_err(ApiError::bad_request)?
        .ok_or_else(|| ApiError::not_found(format!("运行记录不存在: {id}")))?;
    ensure_run_access(&state, &run, &current_user).await?;
    let task = state
        .task_service
        .get_task(run.task_id.as_str())
        .await
        .map_err(ApiError::bad_request)?
        .ok_or_else(|| ApiError::not_found(format!("任务不存在: {}", run.task_id)))?;
    let changes = crate::services::load_task_run_workspace_changes(&state.run_service, &task, &run)
        .await
        .map_err(ApiError::bad_gateway)?;
    Ok(Json(redact_workspace_paths(&state, changes)?))
}

pub(in crate::api) async fn list_run_events(
    Path(id): Path<String>,
    Query(query): Query<RunEventListQuery>,
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
    let events = match (query.after_created_at.as_deref(), query.after_id.as_deref()) {
        (Some(created_at), Some(event_id)) => {
            state
                .run_service
                .list_run_events_after(
                    &id,
                    Some(created_at),
                    Some(event_id),
                    query.limit.unwrap_or(200).clamp(1, 500),
                )
                .await
        }
        (None, None) => state.run_service.list_run_events(&id).await,
        _ => Err("after_created_at and after_id must be supplied together".to_string()),
    }
    .map_err(ApiError::bad_request)?;
    Ok(Json(redact_workspace_paths(&state, events)?))
}

#[derive(Debug, Default, Deserialize)]
pub(in crate::api) struct RunEventListQuery {
    after_created_at: Option<String>,
    after_id: Option<String>,
    limit: Option<usize>,
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
    Ok(Json(redact_workspace_paths(&state, run)?))
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
    Ok((
        StatusCode::CREATED,
        Json(redact_workspace_paths(&state, run)?),
    ))
}

pub(in crate::api) async fn retry_run_workspace_integration(
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
        .retry_run_workspace_integration(&id)
        .await
        .map_err(ApiError::bad_request)?
        .ok_or_else(|| ApiError::conflict("当前运行没有可重试的代码集成冲突"))?;
    Ok(Json(redact_workspace_paths(&state, run)?))
}
