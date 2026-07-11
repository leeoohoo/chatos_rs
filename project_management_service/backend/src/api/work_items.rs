// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::{Extension, Json};
use serde::Deserialize;
use serde_json::{json, Value};

use super::access::{
    ensure_project_writable, require_project_access, require_requirement_access,
    require_work_item_access,
};
use super::ApiError;
use crate::auth::CurrentUser;
use crate::domain::visibility::{non_archived_project_tasks, should_include_archived};
use crate::models::{
    CreateProjectWorkItemRequest, ProjectWorkItemRecord, ProjectWorkItemStatus, RequirementRecord,
    UpdateProjectWorkItemRequest,
};
use crate::services::project_plan;
use crate::state::AppState;

#[derive(Debug, Default, Deserialize)]
pub(in crate::api) struct WorkItemListQuery {
    status: Option<ProjectWorkItemStatus>,
    keyword: Option<String>,
    is_planning_task: Option<bool>,
    include_archived: Option<bool>,
}

pub(in crate::api) async fn list_project_work_items(
    Path(project_id): Path<String>,
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Query(query): Query<WorkItemListQuery>,
) -> Result<Json<Vec<ProjectWorkItemRecord>>, ApiError> {
    require_project_access(&state, &project_id, &user).await?;
    let include_archived = should_include_archived(
        query.include_archived,
        matches!(query.status, Some(ProjectWorkItemStatus::Archived)),
    );
    let mut items = state
        .store
        .list_work_items_by_project(
            &project_id,
            query.status,
            query.is_planning_task,
            query.keyword,
        )
        .await
        .map_err(ApiError::bad_request)?;
    if !include_archived {
        items = non_archived_project_tasks(items);
    }
    Ok(Json(items))
}

#[derive(Debug, Default, Deserialize)]
pub(in crate::api) struct RequirementWorkItemListQuery {
    include_archived: Option<bool>,
    include_dependency_graph: Option<bool>,
}

pub(in crate::api) async fn list_requirement_work_items(
    Path(requirement_id): Path<String>,
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Query(query): Query<RequirementWorkItemListQuery>,
) -> Result<Json<Value>, ApiError> {
    let requirement = require_requirement_access(&state, &requirement_id, &user).await?;
    list_requirement_work_items_response(&state, &requirement, query)
        .await
        .map(Json)
}

pub(in crate::api) async fn list_project_requirement_work_items(
    Path((project_id, requirement_id)): Path<(String, String)>,
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Query(query): Query<RequirementWorkItemListQuery>,
) -> Result<Json<Value>, ApiError> {
    require_project_access(&state, &project_id, &user).await?;
    let requirement = require_requirement_access(&state, &requirement_id, &user).await?;
    if requirement.project_id != project_id {
        return Err(ApiError::not_found(format!(
            "requirement does not belong to project: {requirement_id}"
        )));
    }
    list_requirement_work_items_response(&state, &requirement, query)
        .await
        .map(Json)
}

async fn list_requirement_work_items_response(
    state: &AppState,
    requirement: &RequirementRecord,
    query: RequirementWorkItemListQuery,
) -> Result<Value, ApiError> {
    let mut items = state
        .store
        .list_work_items_by_requirement(&requirement.id)
        .await
        .map_err(ApiError::bad_request)?;
    if !query.include_archived.unwrap_or(false) {
        items = non_archived_project_tasks(items);
    }
    if !query.include_dependency_graph.unwrap_or(false) {
        return Ok(json!(items));
    }

    let dependency_graph =
        project_plan::requirement_work_items_dependency_graph(&state.store, requirement, &items)
            .await
            .map_err(ApiError::bad_request)?;
    let work_items = json!(items);
    let dependency_graph = json!(dependency_graph);
    Ok(json!({
        "work_items": work_items.clone(),
        "workItems": work_items,
        "dependency_graph": dependency_graph.clone(),
        "dependencyGraph": dependency_graph,
    }))
}

pub(in crate::api) async fn create_work_item(
    Path(requirement_id): Path<String>,
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Json(input): Json<CreateProjectWorkItemRequest>,
) -> Result<(StatusCode, Json<ProjectWorkItemRecord>), ApiError> {
    let requirement = require_requirement_access(&state, &requirement_id, &user).await?;
    let project = require_project_access(&state, &requirement.project_id, &user).await?;
    ensure_project_writable(&project)?;
    let item = state
        .store
        .create_work_item(&requirement, input, &user)
        .await
        .map_err(ApiError::bad_request)?;
    Ok((StatusCode::CREATED, Json(item)))
}

pub(in crate::api) async fn get_work_item(
    Path(work_item_id): Path<String>,
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
) -> Result<Json<ProjectWorkItemRecord>, ApiError> {
    let item = require_work_item_access(&state, &work_item_id, &user).await?;
    Ok(Json(item))
}

pub(in crate::api) async fn update_work_item(
    Path(work_item_id): Path<String>,
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Json(input): Json<UpdateProjectWorkItemRequest>,
) -> Result<Json<ProjectWorkItemRecord>, ApiError> {
    let item = require_work_item_access(&state, &work_item_id, &user).await?;
    let project = require_project_access(&state, &item.project_id, &user).await?;
    ensure_project_writable(&project)?;
    state
        .store
        .update_work_item(&work_item_id, input)
        .await
        .map_err(ApiError::bad_request)?
        .map(Json)
        .ok_or_else(|| ApiError::not_found(format!("项目工作项不存在: {work_item_id}")))
}

pub(in crate::api) async fn delete_work_item(
    Path(work_item_id): Path<String>,
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
) -> Result<Json<ProjectWorkItemRecord>, ApiError> {
    let item = require_work_item_access(&state, &work_item_id, &user).await?;
    let project = require_project_access(&state, &item.project_id, &user).await?;
    ensure_project_writable(&project)?;
    state
        .store
        .archive_work_item(&work_item_id)
        .await
        .map_err(ApiError::bad_request)?
        .map(Json)
        .ok_or_else(|| ApiError::not_found(format!("项目工作项不存在: {work_item_id}")))
}
