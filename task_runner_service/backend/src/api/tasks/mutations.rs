// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

pub(in crate::api) async fn create_task(
    State(state): State<AppState>,
    Extension(current_user): Extension<CurrentUser>,
    headers: HeaderMap,
    Json(input): Json<CreateTaskRequest>,
) -> Result<(StatusCode, Json<TaskRecord>), ApiError> {
    let source_context = task_source_context_from_headers(&headers, input.project_id.clone());
    let task = state
        .task_service
        .create_task(input, Some(&current_user), source_context)
        .await
        .map_err(ApiError::bad_request)?;
    Ok((
        StatusCode::CREATED,
        Json(redact_workspace_paths(&state, task)?),
    ))
}

fn task_source_context_from_headers(
    headers: &HeaderMap,
    project_id: Option<String>,
) -> Option<TaskSourceContext> {
    let source_session_id = header_text(headers, "x-chatos-session-id")
        .or_else(|| header_text(headers, "x-chatos-source-session-id"));
    let source_user_message_id = header_text(headers, "x-chatos-user-message-id")
        .or_else(|| header_text(headers, "x-chatos-source-user-message-id"));
    let source_turn_id = header_text(headers, "x-chatos-turn-id")
        .or_else(|| header_text(headers, "x-chatos-source-turn-id"));
    if source_session_id.is_none() && source_user_message_id.is_none() && source_turn_id.is_none() {
        return None;
    }
    Some(TaskSourceContext {
        project_id,
        source_session_id,
        source_user_message_id,
        source_turn_id,
        ..TaskSourceContext::default()
    })
}

fn header_text(headers: &HeaderMap, name: &str) -> Option<String> {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

pub(in crate::api) async fn batch_update_task_status(
    State(state): State<AppState>,
    Extension(current_user): Extension<CurrentUser>,
    Json(input): Json<BatchTaskStatusUpdateRequest>,
) -> Result<Json<BatchTaskOperationResponse>, ApiError> {
    if input.status == TaskStatus::Cancelled {
        return Err(ApiError::bad_request("请使用 cancel_task 并提供取消原因"));
    }
    let mut results = Vec::new();
    for task_id in input
        .task_ids
        .into_iter()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .take(500)
    {
        match get_task_for_user(&state, task_id.as_str(), &current_user).await {
            Ok(Some(_)) => match state
                .task_service
                .update_task(
                    task_id.as_str(),
                    UpdateTaskRequest {
                        status: Some(input.status),
                        ..UpdateTaskRequest::default()
                    },
                    Some(&current_user),
                )
                .await
            {
                Ok(Some(_)) => results.push(batch_result(task_id, true, None, None)),
                Ok(None) => results.push(batch_result(
                    task_id,
                    false,
                    Some("任务不存在".to_string()),
                    None,
                )),
                Err(err) => results.push(batch_result(task_id, false, Some(err), None)),
            },
            Ok(None) => results.push(batch_result(
                task_id,
                false,
                Some("任务不存在".to_string()),
                None,
            )),
            Err(err) => results.push(batch_result(task_id, false, Some(err.into_message()), None)),
        }
    }
    Ok(Json(batch_response(results)))
}

pub(in crate::api) async fn batch_delete_tasks(
    State(state): State<AppState>,
    Extension(current_user): Extension<CurrentUser>,
    Json(input): Json<BatchTaskDeleteRequest>,
) -> Result<Json<BatchTaskOperationResponse>, ApiError> {
    let mut results = Vec::new();
    for task_id in input
        .task_ids
        .into_iter()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .take(500)
    {
        match get_task_for_user(&state, task_id.as_str(), &current_user).await {
            Ok(Some(_)) => match state.task_service.delete_task(task_id.as_str()).await {
                Ok(true) => results.push(batch_result(task_id, true, None, None)),
                Ok(false) => results.push(batch_result(
                    task_id,
                    false,
                    Some("任务不存在".to_string()),
                    None,
                )),
                Err(err) => results.push(batch_result(task_id, false, Some(err), None)),
            },
            Ok(None) => results.push(batch_result(
                task_id,
                false,
                Some("任务不存在".to_string()),
                None,
            )),
            Err(err) => results.push(batch_result(task_id, false, Some(err.into_message()), None)),
        }
    }
    Ok(Json(batch_response(results)))
}

pub(in crate::api) async fn batch_start_task_runs(
    State(state): State<AppState>,
    Extension(current_user): Extension<CurrentUser>,
    Json(input): Json<BatchTaskRunRequest>,
) -> Result<Json<BatchTaskOperationResponse>, ApiError> {
    for task_id in &input.task_ids {
        let task_id = task_id.trim();
        if !task_id.is_empty() {
            get_task_for_user(&state, task_id, &current_user)
                .await?
                .ok_or_else(|| ApiError::not_found(format!("任务不存在: {task_id}")))?;
        }
    }
    let result = state
        .run_service
        .batch_start_runs_for_user(input, &current_user)
        .await
        .map_err(ApiError::bad_request)?;
    Ok(Json(redact_workspace_paths(&state, result)?))
}

pub(in crate::api) async fn get_task(
    Path(id): Path<String>,
    State(state): State<AppState>,
    Extension(current_user): Extension<CurrentUser>,
) -> Result<Json<TaskRecord>, ApiError> {
    get_task_for_user(&state, &id, &current_user)
        .await?
        .map(|task| redact_workspace_paths(&state, task).map(Json))
        .transpose()?
        .ok_or_else(|| ApiError::not_found(format!("task not found: {id}")))
}

pub(in crate::api) async fn update_task(
    Path(id): Path<String>,
    State(state): State<AppState>,
    Extension(current_user): Extension<CurrentUser>,
    Json(input): Json<UpdateTaskRequest>,
) -> Result<Json<TaskRecord>, ApiError> {
    get_task_for_user(&state, &id, &current_user)
        .await?
        .ok_or_else(|| ApiError::not_found(format!("task not found: {id}")))?;
    let task = state
        .task_service
        .update_task(&id, input, Some(&current_user))
        .await
        .map_err(ApiError::bad_request)?
        .ok_or_else(|| ApiError::not_found(format!("task not found: {id}")))?;
    Ok(Json(redact_workspace_paths(&state, task)?))
}

pub(in crate::api) async fn delete_task(
    Path(id): Path<String>,
    State(state): State<AppState>,
    Extension(current_user): Extension<CurrentUser>,
) -> Result<StatusCode, ApiError> {
    get_task_for_user(&state, &id, &current_user)
        .await?
        .ok_or_else(|| ApiError::not_found(format!("task not found: {id}")))?;
    if state
        .task_service
        .delete_task(&id)
        .await
        .map_err(ApiError::bad_request)?
    {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ApiError::not_found(format!("task not found: {id}")))
    }
}

pub(in crate::api) async fn cancel_task(
    Path(id): Path<String>,
    State(state): State<AppState>,
    Extension(current_user): Extension<CurrentUser>,
    Json(input): Json<CancelTaskRequest>,
) -> Result<Json<CancelTaskResponse>, ApiError> {
    get_task_for_user(&state, &id, &current_user)
        .await?
        .ok_or_else(|| ApiError::not_found(format!("task not found: {id}")))?;
    let result = state
        .task_service
        .cancel_task(&id, input, Some(&current_user))
        .await
        .map_err(ApiError::bad_request)?
        .ok_or_else(|| ApiError::not_found(format!("task not found: {id}")))?;
    Ok(Json(redact_workspace_paths(&state, result)?))
}

pub(in crate::api) async fn update_task_mcp(
    Path(id): Path<String>,
    State(state): State<AppState>,
    Extension(current_user): Extension<CurrentUser>,
    Json(input): Json<UpdateTaskMcpRequest>,
) -> Result<Json<TaskRecord>, ApiError> {
    get_task_for_user(&state, &id, &current_user)
        .await?
        .ok_or_else(|| ApiError::not_found(format!("task not found: {id}")))?;
    let task = state
        .task_service
        .update_task_mcp(&id, input, Some(&current_user))
        .await
        .map_err(ApiError::bad_request)?
        .ok_or_else(|| ApiError::not_found(format!("task not found: {id}")))?;
    Ok(Json(redact_workspace_paths(&state, task)?))
}

pub(in crate::api) async fn record_task_process(
    Path(id): Path<String>,
    State(state): State<AppState>,
    Extension(current_user): Extension<CurrentUser>,
    Json(input): Json<RecordTaskProcessRequest>,
) -> Result<Json<TaskRecord>, ApiError> {
    get_task_for_user(&state, &id, &current_user)
        .await?
        .ok_or_else(|| ApiError::not_found(format!("task not found: {id}")))?;
    let task = state
        .task_service
        .record_task_process(&id, input)
        .await
        .map_err(ApiError::bad_request)?
        .ok_or_else(|| ApiError::not_found(format!("task not found: {id}")))?;
    Ok(Json(redact_workspace_paths(&state, task)?))
}

pub(in crate::api) async fn preview_task_mcp_prompt(
    Path(id): Path<String>,
    State(state): State<AppState>,
    Extension(current_user): Extension<CurrentUser>,
) -> Result<Json<McpPromptPreviewResponse>, ApiError> {
    get_task_for_user(&state, &id, &current_user)
        .await?
        .ok_or_else(|| ApiError::not_found(format!("task not found: {id}")))?;
    let preview = state
        .mcp_catalog_service
        .preview_task_prompt(&id)
        .await
        .map_err(ApiError::bad_request)?
        .ok_or_else(|| ApiError::not_found(format!("task not found: {id}")))?;
    Ok(Json(redact_workspace_paths(&state, preview)?))
}

fn batch_result(
    task_id: String,
    ok: bool,
    message: Option<String>,
    run_id: Option<String>,
) -> BatchTaskOperationItem {
    BatchTaskOperationItem {
        task_id,
        ok,
        message,
        run_id,
    }
}

fn batch_response(results: Vec<BatchTaskOperationItem>) -> BatchTaskOperationResponse {
    let total = results.len();
    let succeeded = results.iter().filter(|item| item.ok).count();
    BatchTaskOperationResponse {
        total,
        succeeded,
        failed: total.saturating_sub(succeeded),
        results,
    }
}
