// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::extract::{Path, Query, State};
use axum::Json;
use serde::Deserialize;

use crate::local_runtime::project_management::CreateLocalRequirementInput;
use crate::LocalRuntime;

use super::super::context::owner_context;
use super::super::error::LocalRuntimeApiError;
use super::{one_of, optional, required};

pub(super) const REQUIREMENT_TYPES: &[&str] = &["requirement", "change", "bug_fix"];
pub(super) const REQUIREMENT_STATUSES: &[&str] = &[
    "draft",
    "reviewing",
    "approved",
    "in_progress",
    "blocked",
    "failed",
    "done",
    "cancelled",
    "archived",
];

#[derive(Debug, Default, Deserialize)]
pub(super) struct RequirementQuery {
    include_archived: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub(super) struct CreateRequirementRequest {
    parent_requirement_id: Option<String>,
    requirement_type: Option<String>,
    title: String,
    summary: Option<String>,
    detail: Option<String>,
    business_value: Option<String>,
    acceptance_criteria: Option<String>,
    source: Option<String>,
    priority: Option<i64>,
    status: Option<String>,
    assignee_user_id: Option<String>,
}

pub(super) async fn list_requirements(
    Path(project_id): Path<String>,
    Query(query): Query<RequirementQuery>,
    State(runtime): State<LocalRuntime>,
) -> Result<Json<serde_json::Value>, LocalRuntimeApiError> {
    let owner = owner_context(&runtime).await?;
    let project_id = required(project_id, "project_id")?;
    let records = runtime
        .local_database()?
        .list_local_requirements(
            owner.owner_user_id.as_str(),
            project_id.as_str(),
            query.include_archived.unwrap_or(false),
        )
        .await?;
    Ok(Json(serde_json::json!(records)))
}

pub(super) async fn create_requirement(
    Path(project_id): Path<String>,
    State(runtime): State<LocalRuntime>,
    Json(request): Json<CreateRequirementRequest>,
) -> Result<Json<serde_json::Value>, LocalRuntimeApiError> {
    let owner = owner_context(&runtime).await?;
    let record = runtime
        .local_database()?
        .create_local_requirement(CreateLocalRequirementInput {
            project_id: required(project_id, "project_id")?,
            owner_user_id: owner.owner_user_id,
            parent_requirement_id: optional(request.parent_requirement_id),
            requirement_type: one_of(request.requirement_type, "requirement", REQUIREMENT_TYPES),
            title: required(request.title, "title")?,
            summary: optional(request.summary),
            detail: optional(request.detail),
            business_value: optional(request.business_value),
            acceptance_criteria: optional(request.acceptance_criteria),
            source: optional(request.source),
            priority: request.priority.unwrap_or_default().clamp(-100, 100),
            status: one_of(request.status, "draft", REQUIREMENT_STATUSES),
            assignee_user_id: optional(request.assignee_user_id),
        })
        .await?;
    Ok(Json(serde_json::json!(record)))
}
