use super::*;

pub(in crate::api) async fn create_task(
    State(state): State<AppState>,
    Extension(current_user): Extension<CurrentUser>,
    Json(input): Json<CreateTaskRequest>,
) -> Result<(StatusCode, Json<TaskRecord>), ApiError> {
    let task = state
        .task_service
        .create_task(input, Some(&current_user), None)
        .await
        .map_err(ApiError::bad_request)?;
    Ok((StatusCode::CREATED, Json(task)))
}

pub(in crate::api) async fn batch_update_task_status(
    State(state): State<AppState>,
    Json(input): Json<BatchTaskStatusUpdateRequest>,
) -> Result<Json<BatchTaskOperationResponse>, ApiError> {
    let result = state
        .task_service
        .batch_update_status(input)
        .await
        .map_err(ApiError::bad_request)?;
    Ok(Json(result))
}

pub(in crate::api) async fn batch_delete_tasks(
    State(state): State<AppState>,
    Json(input): Json<BatchTaskDeleteRequest>,
) -> Result<Json<BatchTaskOperationResponse>, ApiError> {
    let result = state
        .task_service
        .batch_delete_tasks(input)
        .await
        .map_err(ApiError::bad_request)?;
    Ok(Json(result))
}

pub(in crate::api) async fn batch_start_task_runs(
    State(state): State<AppState>,
    Json(input): Json<BatchTaskRunRequest>,
) -> Result<Json<BatchTaskOperationResponse>, ApiError> {
    let result = state
        .run_service
        .batch_start_runs(input)
        .await
        .map_err(ApiError::bad_request)?;
    Ok(Json(result))
}

pub(in crate::api) async fn get_task(
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> Result<Json<TaskRecord>, ApiError> {
    state
        .task_service
        .get_task(&id)
        .await
        .map_err(ApiError::bad_request)?
        .map(Json)
        .ok_or_else(|| ApiError::not_found(format!("任务不存在: {id}")))
}

pub(in crate::api) async fn update_task(
    Path(id): Path<String>,
    State(state): State<AppState>,
    Json(input): Json<UpdateTaskRequest>,
) -> Result<Json<TaskRecord>, ApiError> {
    let task = state
        .task_service
        .update_task(&id, input)
        .await
        .map_err(ApiError::bad_request)?
        .ok_or_else(|| ApiError::not_found(format!("任务不存在: {id}")))?;
    Ok(Json(task))
}

pub(in crate::api) async fn delete_task(
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> Result<StatusCode, ApiError> {
    if state
        .task_service
        .delete_task(&id)
        .await
        .map_err(ApiError::bad_request)?
    {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ApiError::not_found(format!("任务不存在: {id}")))
    }
}

pub(in crate::api) async fn update_task_mcp(
    Path(id): Path<String>,
    State(state): State<AppState>,
    Json(input): Json<UpdateTaskMcpRequest>,
) -> Result<Json<TaskRecord>, ApiError> {
    let task = state
        .task_service
        .update_task_mcp(&id, input)
        .await
        .map_err(ApiError::bad_request)?
        .ok_or_else(|| ApiError::not_found(format!("任务不存在: {id}")))?;
    Ok(Json(task))
}

pub(in crate::api) async fn record_task_process(
    Path(id): Path<String>,
    State(state): State<AppState>,
    Json(input): Json<RecordTaskProcessRequest>,
) -> Result<Json<TaskRecord>, ApiError> {
    let task = state
        .task_service
        .record_task_process(&id, input)
        .await
        .map_err(ApiError::bad_request)?
        .ok_or_else(|| ApiError::not_found(format!("任务不存在: {id}")))?;
    Ok(Json(task))
}

pub(in crate::api) async fn preview_task_mcp_prompt(
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> Result<Json<McpPromptPreviewResponse>, ApiError> {
    let preview = state
        .mcp_catalog_service
        .preview_task_prompt(&id)
        .await
        .map_err(ApiError::bad_request)?
        .ok_or_else(|| ApiError::not_found(format!("任务不存在: {id}")))?;
    Ok(Json(preview))
}
