// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::extract::{Path, State};
use axum::Json;
use serde::Deserialize;

use crate::local_runtime::project_management::UpdateLocalWorkItemInput;
use crate::LocalRuntime;

use super::super::context::owner_context;
use super::super::error::LocalRuntimeApiError;
use super::work_items::WORK_ITEM_STATUSES;
use super::{optional, optional_one_of, required};

#[derive(Debug, Default, Deserialize)]
pub(super) struct UpdateWorkItemRequest {
    requirement_id: Option<String>,
    title: Option<String>,
    description: Option<String>,
    status: Option<String>,
    priority: Option<i64>,
    assignee_user_id: Option<String>,
    estimate_points: Option<i64>,
    due_at: Option<String>,
    sort_order: Option<i64>,
    tags: Option<Vec<String>>,
    is_planning_task: Option<bool>,
}

#[derive(Debug, Default, Deserialize)]
pub(super) struct SetDependenciesRequest {
    prerequisite_work_item_ids: Vec<String>,
}

pub(super) async fn update_work_item(
    Path(work_item_id): Path<String>,
    State(runtime): State<LocalRuntime>,
    Json(request): Json<UpdateWorkItemRequest>,
) -> Result<Json<serde_json::Value>, LocalRuntimeApiError> {
    let owner = owner_context(&runtime).await?;
    let record = runtime
        .local_database()?
        .update_local_work_item(
            owner.owner_user_id.as_str(),
            required(work_item_id, "work_item_id")?.as_str(),
            UpdateLocalWorkItemInput {
                requirement_id: optional(request.requirement_id),
                title: optional(request.title),
                description: optional(request.description),
                status: optional_one_of(request.status, WORK_ITEM_STATUSES),
                priority: request.priority.map(|value| value.clamp(-100, 100)),
                assignee_user_id: optional(request.assignee_user_id),
                estimate_points: request.estimate_points.map(|value| value.clamp(0, 10_000)),
                due_at: optional(request.due_at),
                sort_order: request.sort_order,
                tags: request.tags.map(normalize_tags),
                is_planning_task: request.is_planning_task,
            },
        )
        .await?
        .ok_or_else(work_item_not_found)?;
    Ok(Json(serde_json::json!(record)))
}

pub(super) async fn archive_work_item(
    Path(work_item_id): Path<String>,
    State(runtime): State<LocalRuntime>,
) -> Result<Json<serde_json::Value>, LocalRuntimeApiError> {
    let owner = owner_context(&runtime).await?;
    let record = runtime
        .local_database()?
        .archive_local_work_item(
            owner.owner_user_id.as_str(),
            required(work_item_id, "work_item_id")?.as_str(),
        )
        .await?
        .ok_or_else(work_item_not_found)?;
    Ok(Json(serde_json::json!(record)))
}

pub(super) async fn list_dependencies(
    Path((project_id, work_item_id)): Path<(String, String)>,
    State(runtime): State<LocalRuntime>,
) -> Result<Json<serde_json::Value>, LocalRuntimeApiError> {
    let owner = owner_context(&runtime).await?;
    let project_id = required(project_id, "project_id")?;
    let work_item_id = required(work_item_id, "work_item_id")?;
    let work_item = runtime
        .local_database()?
        .get_local_work_item(owner.owner_user_id.as_str(), work_item_id.as_str())
        .await?
        .ok_or_else(work_item_not_found)?;
    if work_item.project_id != project_id {
        return Err(work_item_not_found());
    }
    let records = runtime
        .local_database()?
        .list_local_work_item_dependencies(work_item_id.as_str())
        .await?;
    Ok(Json(serde_json::json!(records)))
}

pub(super) async fn set_dependencies(
    Path((project_id, work_item_id)): Path<(String, String)>,
    State(runtime): State<LocalRuntime>,
    Json(request): Json<SetDependenciesRequest>,
) -> Result<Json<serde_json::Value>, LocalRuntimeApiError> {
    let owner = owner_context(&runtime).await?;
    let records = runtime
        .local_database()?
        .set_local_work_item_dependencies(
            owner.owner_user_id.as_str(),
            required(project_id, "project_id")?.as_str(),
            required(work_item_id, "work_item_id")?.as_str(),
            request.prerequisite_work_item_ids,
        )
        .await?;
    Ok(Json(serde_json::json!(records)))
}

fn normalize_tags(tags: Vec<String>) -> Vec<String> {
    tags.into_iter()
        .filter_map(|tag| optional(Some(tag)))
        .take(50)
        .collect()
}

fn work_item_not_found() -> LocalRuntimeApiError {
    LocalRuntimeApiError::not_found(
        "local_project_work_item_not_found",
        "Local project work item was not found",
    )
}
