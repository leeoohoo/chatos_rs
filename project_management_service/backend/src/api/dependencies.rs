// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::extract::{Path, State};
use axum::{Extension, Json};

use super::access::{
    ensure_project_writable, require_project_access, require_requirement_access,
    require_work_item_access,
};
use super::ApiError;
use crate::auth::CurrentUser;
use crate::models::{
    RequirementDependencyRecord, SetRequirementDependenciesRequest, SetWorkItemDependenciesRequest,
    WorkItemDependencyRecord,
};
use crate::state::AppState;

pub(in crate::api) async fn list_requirement_dependencies(
    Path(requirement_id): Path<String>,
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
) -> Result<Json<Vec<RequirementDependencyRecord>>, ApiError> {
    require_requirement_access(&state, &requirement_id, &user).await?;
    state
        .store
        .list_requirement_dependencies(&requirement_id)
        .await
        .map(Json)
        .map_err(ApiError::bad_request)
}

pub(in crate::api) async fn set_requirement_dependencies(
    Path(requirement_id): Path<String>,
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Json(input): Json<SetRequirementDependenciesRequest>,
) -> Result<Json<Vec<RequirementDependencyRecord>>, ApiError> {
    let requirement = require_requirement_access(&state, &requirement_id, &user).await?;
    let project = require_project_access(&state, &requirement.project_id, &user).await?;
    ensure_project_writable(&project)?;
    state
        .store
        .set_requirement_dependencies(&requirement_id, input.prerequisite_requirement_ids)
        .await
        .map_err(ApiError::bad_request)?;
    state
        .store
        .list_requirement_dependencies(&requirement_id)
        .await
        .map(Json)
        .map_err(ApiError::bad_request)
}

pub(in crate::api) async fn list_work_item_dependencies(
    Path(work_item_id): Path<String>,
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
) -> Result<Json<Vec<WorkItemDependencyRecord>>, ApiError> {
    require_work_item_access(&state, &work_item_id, &user).await?;
    state
        .store
        .list_work_item_dependencies(&work_item_id)
        .await
        .map(Json)
        .map_err(ApiError::bad_request)
}

pub(in crate::api) async fn set_work_item_dependencies(
    Path(work_item_id): Path<String>,
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Json(input): Json<SetWorkItemDependenciesRequest>,
) -> Result<Json<Vec<WorkItemDependencyRecord>>, ApiError> {
    let item = require_work_item_access(&state, &work_item_id, &user).await?;
    let project = require_project_access(&state, &item.project_id, &user).await?;
    ensure_project_writable(&project)?;
    state
        .store
        .set_work_item_dependencies(&work_item_id, input.prerequisite_work_item_ids)
        .await
        .map_err(ApiError::bad_request)?;
    state
        .store
        .list_work_item_dependencies(&work_item_id)
        .await
        .map(Json)
        .map_err(ApiError::bad_request)
}
