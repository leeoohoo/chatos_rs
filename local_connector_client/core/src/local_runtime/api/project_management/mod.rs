// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

mod documents;
mod plan;
mod requirement_mutations;
mod requirements;
mod work_item_mutations;
mod work_items;

use axum::routing::get;
use axum::Router;

use crate::LocalRuntime;

pub(super) fn router() -> Router<LocalRuntime> {
    Router::new()
        .route(
            "/api/local/runtime/projects/{project_id}/plan",
            get(plan::get_project_plan),
        )
        .route(
            "/api/local/runtime/projects/{project_id}/requirements",
            get(requirements::list_requirements).post(requirements::create_requirement),
        )
        .route(
            "/api/local/runtime/projects/{project_id}/requirements/{requirement_id}/work-items",
            get(work_items::list_work_items).post(work_items::create_work_item),
        )
        .route(
            "/api/local/runtime/projects/{project_id}/requirements/{requirement_id}/documents",
            get(documents::list_documents).post(documents::create_document),
        )
        .route(
            "/api/local/runtime/projects/{project_id}/requirements/{requirement_id}/documents/{document_id}",
            axum::routing::put(documents::update_document),
        )
        .route(
            "/api/local/runtime/requirements/{requirement_id}",
            axum::routing::patch(requirement_mutations::update_requirement)
                .delete(requirement_mutations::archive_requirement),
        )
        .route(
            "/api/local/runtime/projects/{project_id}/requirements/{requirement_id}/dependencies",
            get(requirement_mutations::list_dependencies)
                .put(requirement_mutations::set_dependencies),
        )
        .route(
            "/api/local/runtime/work-items/{work_item_id}",
            axum::routing::patch(work_item_mutations::update_work_item)
                .delete(work_item_mutations::archive_work_item),
        )
        .route(
            "/api/local/runtime/projects/{project_id}/work-items/{work_item_id}/dependencies",
            get(work_item_mutations::list_dependencies).put(work_item_mutations::set_dependencies),
        )
}

pub(super) fn required(
    value: String,
    field: &str,
) -> Result<String, super::error::LocalRuntimeApiError> {
    let value = value.trim().to_string();
    if value.is_empty() {
        return Err(super::error::LocalRuntimeApiError::bad_request(
            "local_runtime_invalid_request",
            format!("{field} is required"),
        ));
    }
    Ok(value)
}

pub(super) fn optional(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

pub(super) fn one_of(value: Option<String>, default: &str, allowed: &[&str]) -> String {
    let value = optional(value).unwrap_or_else(|| default.to_string());
    if allowed.contains(&value.as_str()) {
        value
    } else {
        default.to_string()
    }
}

pub(super) fn optional_one_of(value: Option<String>, allowed: &[&str]) -> Option<String> {
    optional(value).filter(|value| allowed.contains(&value.as_str()))
}
