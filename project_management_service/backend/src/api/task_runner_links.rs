// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::{Extension, Json};

use super::access::{ensure_project_writable, require_project_access, require_work_item_access};
use super::ApiError;
use crate::auth::CurrentUser;
use crate::models::{LinkTaskRunnerTaskRequest, ProjectWorkItemTaskRunnerLinkRecord};
use crate::state::AppState;

pub(in crate::api) async fn list_task_runner_links(
    Path(work_item_id): Path<String>,
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
) -> Result<Json<Vec<ProjectWorkItemTaskRunnerLinkRecord>>, ApiError> {
    require_work_item_access(&state, &work_item_id, &user).await?;
    state
        .store
        .list_task_runner_links(&work_item_id)
        .await
        .map(Json)
        .map_err(ApiError::bad_request)
}

pub(in crate::api) async fn link_task_runner_task(
    Path(work_item_id): Path<String>,
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Json(input): Json<LinkTaskRunnerTaskRequest>,
) -> Result<(StatusCode, Json<ProjectWorkItemTaskRunnerLinkRecord>), ApiError> {
    let item = require_work_item_access(&state, &work_item_id, &user).await?;
    let project = require_project_access(&state, &item.project_id, &user).await?;
    ensure_project_writable(&project)?;
    let link = state
        .store
        .upsert_task_runner_link(&work_item_id, input)
        .await
        .map_err(ApiError::bad_request)?;
    Ok((StatusCode::CREATED, Json(link)))
}

pub(in crate::api) async fn delete_task_runner_link(
    Path((work_item_id, link_id)): Path<(String, String)>,
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
) -> Result<StatusCode, ApiError> {
    let item = require_work_item_access(&state, &work_item_id, &user).await?;
    let project = require_project_access(&state, &item.project_id, &user).await?;
    ensure_project_writable(&project)?;
    let deleted = state
        .store
        .delete_task_runner_link(&work_item_id, &link_id)
        .await
        .map_err(ApiError::bad_request)?;
    if deleted {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ApiError::not_found(format!(
            "TaskRunner 关联不存在: {link_id}"
        )))
    }
}
