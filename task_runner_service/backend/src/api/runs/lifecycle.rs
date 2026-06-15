use super::*;

pub(in crate::api) async fn start_task_run(
    Path(id): Path<String>,
    State(state): State<AppState>,
    Json(input): Json<StartTaskRunRequest>,
) -> Result<(StatusCode, Json<TaskRunRecord>), ApiError> {
    let run = state
        .run_service
        .start_run(&id, input)
        .await
        .map_err(ApiError::bad_request)?;
    Ok((StatusCode::CREATED, Json(run)))
}

pub(in crate::api) async fn get_run(
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> Result<Json<TaskRunRecord>, ApiError> {
    state
        .run_service
        .get_run(&id)
        .await
        .map_err(ApiError::bad_request)?
        .map(Json)
        .ok_or_else(|| ApiError::not_found(format!("运行记录不存在: {id}")))
}

pub(in crate::api) async fn list_run_events(
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> Result<Json<Vec<TaskRunEventRecord>>, ApiError> {
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
) -> Result<Json<TaskRunRecord>, ApiError> {
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
) -> Result<(StatusCode, Json<TaskRunRecord>), ApiError> {
    let run = state
        .run_service
        .retry_run(&id)
        .await
        .map_err(ApiError::bad_request)?
        .ok_or_else(|| ApiError::not_found(format!("运行记录不存在: {id}")))?;
    Ok((StatusCode::CREATED, Json(run)))
}
