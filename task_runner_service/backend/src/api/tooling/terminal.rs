// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

#[derive(Debug, Default, Deserialize)]
pub(in crate::api) struct ToolingTerminalProcessesQuery {
    user_id: Option<String>,
    project_id: Option<String>,
    include_exited: Option<bool>,
    limit: Option<usize>,
}

#[derive(Debug, Default, Deserialize)]
pub(in crate::api) struct ToolingTerminalLogsQuery {
    user_id: Option<String>,
    project_id: Option<String>,
    offset: Option<i64>,
    limit: Option<i64>,
}

#[derive(Debug, Default, Deserialize)]
pub(in crate::api) struct KillTerminalProcessRequest {
    user_id: Option<String>,
    project_id: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
pub(in crate::api) struct WriteTerminalProcessRequest {
    user_id: Option<String>,
    project_id: Option<String>,
    data: String,
    submit: Option<bool>,
}

pub(in crate::api) async fn list_terminal_processes(
    State(state): State<AppState>,
    Query(query): Query<ToolingTerminalProcessesQuery>,
) -> Result<Json<Value>, ApiError> {
    let response = state
        .tooling_state_service
        .list_terminal_processes(
            query.user_id,
            query.project_id,
            query.include_exited.unwrap_or(true),
            query.limit.unwrap_or(50),
        )
        .await
        .map_err(ApiError::bad_request)?;
    Ok(Json(response))
}

pub(in crate::api) async fn get_terminal_process_logs(
    Path(id): Path<String>,
    State(state): State<AppState>,
    Query(query): Query<ToolingTerminalLogsQuery>,
) -> Result<Json<Value>, ApiError> {
    let response = state
        .tooling_state_service
        .get_terminal_process_logs(
            &id,
            query.user_id,
            query.project_id,
            query.offset,
            query.limit,
        )
        .await
        .map_err(ApiError::bad_request)?;
    Ok(Json(response))
}

pub(in crate::api) async fn kill_terminal_process(
    Path(id): Path<String>,
    State(state): State<AppState>,
    Json(input): Json<KillTerminalProcessRequest>,
) -> Result<Json<Value>, ApiError> {
    let response = state
        .tooling_state_service
        .kill_terminal_process(&id, input.user_id, input.project_id)
        .await
        .map_err(ApiError::bad_request)?;
    Ok(Json(response))
}

pub(in crate::api) async fn write_terminal_process(
    Path(id): Path<String>,
    State(state): State<AppState>,
    Json(input): Json<WriteTerminalProcessRequest>,
) -> Result<Json<Value>, ApiError> {
    let response = state
        .tooling_state_service
        .write_terminal_process(
            &id,
            input.user_id,
            input.project_id,
            input.data,
            input.submit.unwrap_or(true),
        )
        .await
        .map_err(ApiError::bad_request)?;
    Ok(Json(response))
}
