// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::extract::{Path, State};
use axum::Json;
use serde::Deserialize;

use crate::local_runtime::project_management::UpdateLocalRequirementInput;
use crate::LocalRuntime;

use super::super::context::owner_context;
use super::super::error::LocalRuntimeApiError;
use super::requirements::{REQUIREMENT_STATUSES, REQUIREMENT_TYPES};
use super::{optional, optional_one_of, required};

#[derive(Debug, Default, Deserialize)]
pub(super) struct UpdateRequirementRequest {
    parent_requirement_id: Option<String>,
    requirement_type: Option<String>,
    title: Option<String>,
    summary: Option<String>,
    detail: Option<String>,
    business_value: Option<String>,
    acceptance_criteria: Option<String>,
    source: Option<String>,
    priority: Option<i64>,
    status: Option<String>,
    assignee_user_id: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
pub(super) struct SetDependenciesRequest {
    prerequisite_requirement_ids: Vec<String>,
}

pub(super) async fn update_requirement(
    Path(requirement_id): Path<String>,
    State(runtime): State<LocalRuntime>,
    Json(request): Json<UpdateRequirementRequest>,
) -> Result<Json<serde_json::Value>, LocalRuntimeApiError> {
    let owner = owner_context(&runtime).await?;
    let record = runtime
        .local_database()?
        .update_local_requirement(
            owner.owner_user_id.as_str(),
            required(requirement_id, "requirement_id")?.as_str(),
            UpdateLocalRequirementInput {
                parent_requirement_id: optional(request.parent_requirement_id),
                requirement_type: optional_one_of(request.requirement_type, REQUIREMENT_TYPES),
                title: optional(request.title),
                summary: optional(request.summary),
                detail: optional(request.detail),
                business_value: optional(request.business_value),
                acceptance_criteria: optional(request.acceptance_criteria),
                source: optional(request.source),
                priority: request.priority.map(|value| value.clamp(-100, 100)),
                status: optional_one_of(request.status, REQUIREMENT_STATUSES),
                assignee_user_id: optional(request.assignee_user_id),
            },
        )
        .await?
        .ok_or_else(requirement_not_found)?;
    Ok(Json(serde_json::json!(record)))
}

pub(super) async fn archive_requirement(
    Path(requirement_id): Path<String>,
    State(runtime): State<LocalRuntime>,
) -> Result<Json<serde_json::Value>, LocalRuntimeApiError> {
    let owner = owner_context(&runtime).await?;
    let record = runtime
        .local_database()?
        .archive_local_requirement(
            owner.owner_user_id.as_str(),
            required(requirement_id, "requirement_id")?.as_str(),
        )
        .await?
        .ok_or_else(requirement_not_found)?;
    Ok(Json(serde_json::json!(record)))
}

pub(super) async fn list_dependencies(
    Path((project_id, requirement_id)): Path<(String, String)>,
    State(runtime): State<LocalRuntime>,
) -> Result<Json<serde_json::Value>, LocalRuntimeApiError> {
    let owner = owner_context(&runtime).await?;
    let project_id = required(project_id, "project_id")?;
    let requirement_id = required(requirement_id, "requirement_id")?;
    let requirement = runtime
        .local_database()?
        .get_local_requirement(owner.owner_user_id.as_str(), requirement_id.as_str())
        .await?
        .ok_or_else(requirement_not_found)?;
    if requirement.project_id != project_id {
        return Err(requirement_not_found());
    }
    let records = runtime
        .local_database()?
        .list_local_requirement_dependencies(requirement_id.as_str())
        .await?;
    Ok(Json(serde_json::json!(records)))
}

pub(super) async fn set_dependencies(
    Path((project_id, requirement_id)): Path<(String, String)>,
    State(runtime): State<LocalRuntime>,
    Json(request): Json<SetDependenciesRequest>,
) -> Result<Json<serde_json::Value>, LocalRuntimeApiError> {
    let owner = owner_context(&runtime).await?;
    let records = runtime
        .local_database()?
        .set_local_requirement_dependencies(
            owner.owner_user_id.as_str(),
            required(project_id, "project_id")?.as_str(),
            required(requirement_id, "requirement_id")?.as_str(),
            request.prerequisite_requirement_ids,
        )
        .await?;
    Ok(Json(serde_json::json!(records)))
}

fn requirement_not_found() -> LocalRuntimeApiError {
    LocalRuntimeApiError::not_found(
        "local_project_requirement_not_found",
        "Local project requirement was not found",
    )
}
