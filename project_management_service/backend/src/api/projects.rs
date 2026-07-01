// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::{Extension, Json};
use serde::Deserialize;

use super::access::{ensure_project_writable, require_project_access};
use super::ApiError;
use crate::auth::CurrentUser;
use crate::models::{
    now_rfc3339, CreateProjectRequest, ProjectProfileRecord, ProjectRecord, ProjectStatus,
    UpdateProjectRequest, UpsertProjectProfileRequest,
};
use crate::state::AppState;

#[derive(Debug, Default, Deserialize)]
pub(in crate::api) struct ProjectListQuery {
    status: Option<ProjectStatus>,
}

pub(in crate::api) async fn list_projects(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Query(query): Query<ProjectListQuery>,
) -> Result<Json<Vec<ProjectRecord>>, ApiError> {
    state
        .store
        .list_projects(&user, query.status)
        .await
        .map(Json)
        .map_err(ApiError::bad_request)
}

pub(in crate::api) async fn create_project(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Json(input): Json<CreateProjectRequest>,
) -> Result<(StatusCode, Json<ProjectRecord>), ApiError> {
    let project = state
        .store
        .create_project(input, &user)
        .await
        .map_err(ApiError::bad_request)?;
    Ok((StatusCode::CREATED, Json(project)))
}

pub(in crate::api) async fn get_project(
    Path(project_id): Path<String>,
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
) -> Result<Json<ProjectRecord>, ApiError> {
    let project = require_project_access(&state, &project_id, &user).await?;
    Ok(Json(project))
}

pub(in crate::api) async fn update_project(
    Path(project_id): Path<String>,
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Json(input): Json<UpdateProjectRequest>,
) -> Result<Json<ProjectRecord>, ApiError> {
    let project = require_project_access(&state, &project_id, &user).await?;
    ensure_project_writable(&project)?;
    let project = state
        .store
        .update_project(&project_id, input)
        .await
        .map_err(ApiError::bad_request)?
        .ok_or_else(|| ApiError::not_found(format!("项目不存在: {project_id}")))?;
    Ok(Json(project))
}

pub(in crate::api) async fn delete_project(
    Path(project_id): Path<String>,
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
) -> Result<Json<ProjectRecord>, ApiError> {
    let project = require_project_access(&state, &project_id, &user).await?;
    ensure_project_writable(&project)?;
    let project = state
        .store
        .archive_project(&project_id)
        .await
        .map_err(ApiError::bad_request)?
        .ok_or_else(|| ApiError::not_found(format!("项目不存在: {project_id}")))?;
    Ok(Json(project))
}

pub(in crate::api) async fn get_project_profile(
    Path(project_id): Path<String>,
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
) -> Result<Json<ProjectProfileRecord>, ApiError> {
    require_project_access(&state, &project_id, &user).await?;
    let profile = state
        .store
        .get_project_profile(&project_id)
        .await
        .map_err(ApiError::bad_request)?
        .unwrap_or_else(|| {
            let now = now_rfc3339();
            ProjectProfileRecord {
                project_id,
                creator_user_id: None,
                creator_username: None,
                creator_display_name: None,
                owner_user_id: None,
                owner_username: None,
                owner_display_name: None,
                background: None,
                introduction: None,
                created_at: now.clone(),
                updated_at: now,
            }
        });
    Ok(Json(profile))
}

pub(in crate::api) async fn upsert_project_profile(
    Path(project_id): Path<String>,
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Json(input): Json<UpsertProjectProfileRequest>,
) -> Result<Json<ProjectProfileRecord>, ApiError> {
    let project = require_project_access(&state, &project_id, &user).await?;
    ensure_project_writable(&project)?;
    state
        .store
        .upsert_project_profile(&project_id, input, &user)
        .await
        .map(Json)
        .map_err(ApiError::bad_request)
}
