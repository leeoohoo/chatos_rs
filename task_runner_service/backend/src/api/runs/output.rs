// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

#[derive(Debug, Default, Deserialize)]
pub(in crate::api) struct RunOutputChangesQuery {
    limit: Option<usize>,
    offset: Option<usize>,
}

#[derive(Debug, Default, Deserialize)]
pub(in crate::api) struct RunOutputDiffQuery {
    path: String,
}

pub(in crate::api) async fn get_run_output_changes(
    Path(id): Path<String>,
    State(state): State<AppState>,
    Extension(current_user): Extension<CurrentUser>,
    Query(query): Query<RunOutputChangesQuery>,
) -> Result<Json<RunOutputChangesResponse>, ApiError> {
    let run = state
        .run_service
        .get_run(&id)
        .await
        .map_err(ApiError::bad_request)?
        .ok_or_else(|| ApiError::not_found(format!("运行记录不存在: {id}")))?;
    ensure_run_access(&state, &run, &current_user).await?;
    let response = state
        .run_service
        .get_run_output_changes(&id, query.limit, query.offset)
        .await
        .map_err(ApiError::bad_request)?
        .ok_or_else(|| ApiError::not_found(format!("运行记录不存在: {id}")))?;
    Ok(Json(response))
}

pub(in crate::api) async fn get_run_output_diff(
    Path(id): Path<String>,
    State(state): State<AppState>,
    Extension(current_user): Extension<CurrentUser>,
    Query(query): Query<RunOutputDiffQuery>,
) -> Result<Json<RunOutputDiffResponse>, ApiError> {
    let run = state
        .run_service
        .get_run(&id)
        .await
        .map_err(ApiError::bad_request)?
        .ok_or_else(|| ApiError::not_found(format!("运行记录不存在: {id}")))?;
    ensure_run_access(&state, &run, &current_user).await?;
    let response = state
        .run_service
        .get_run_output_diff(&id, query.path.as_str())
        .await
        .map_err(ApiError::bad_request)?
        .ok_or_else(|| ApiError::not_found(format!("运行记录不存在: {id}")))?;
    Ok(Json(response))
}
