// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::extract::{Path, State};
use axum::Json;
use serde::Deserialize;

use crate::local_runtime::project_management::UpsertLocalRequirementDocumentInput;
use crate::LocalRuntime;

use super::super::context::owner_context;
use super::super::error::LocalRuntimeApiError;
use super::{optional, required};

#[derive(Debug, Deserialize)]
pub(super) struct UpsertDocumentRequest {
    doc_type: Option<String>,
    title: Option<String>,
    format: Option<String>,
    content: String,
}

pub(super) async fn list_documents(
    Path((project_id, requirement_id)): Path<(String, String)>,
    State(runtime): State<LocalRuntime>,
) -> Result<Json<serde_json::Value>, LocalRuntimeApiError> {
    let owner = owner_context(&runtime).await?;
    let records = runtime
        .local_database()?
        .list_local_requirement_documents(
            owner.owner_user_id.as_str(),
            required(project_id, "project_id")?.as_str(),
            required(requirement_id, "requirement_id")?.as_str(),
        )
        .await?;
    Ok(Json(serde_json::json!(records)))
}

pub(super) async fn create_document(
    Path((project_id, requirement_id)): Path<(String, String)>,
    State(runtime): State<LocalRuntime>,
    Json(request): Json<UpsertDocumentRequest>,
) -> Result<Json<serde_json::Value>, LocalRuntimeApiError> {
    upsert_document(runtime, project_id, requirement_id, None, request).await
}

pub(super) async fn update_document(
    Path((project_id, requirement_id, document_id)): Path<(String, String, String)>,
    State(runtime): State<LocalRuntime>,
    Json(request): Json<UpsertDocumentRequest>,
) -> Result<Json<serde_json::Value>, LocalRuntimeApiError> {
    upsert_document(
        runtime,
        project_id,
        requirement_id,
        Some(document_id),
        request,
    )
    .await
}

async fn upsert_document(
    runtime: LocalRuntime,
    project_id: String,
    requirement_id: String,
    document_id: Option<String>,
    request: UpsertDocumentRequest,
) -> Result<Json<serde_json::Value>, LocalRuntimeApiError> {
    let owner = owner_context(&runtime).await?;
    let project_id = required(project_id, "project_id")?;
    let requirement_id = required(requirement_id, "requirement_id")?;
    let requirement = runtime
        .local_database()?
        .get_local_requirement(owner.owner_user_id.as_str(), requirement_id.as_str())
        .await?
        .ok_or_else(|| {
            LocalRuntimeApiError::not_found(
                "local_project_requirement_not_found",
                "Local project requirement was not found",
            )
        })?;
    if requirement.project_id != project_id {
        return Err(LocalRuntimeApiError::not_found(
            "local_project_requirement_not_found",
            "Local project requirement was not found",
        ));
    }
    let record = runtime
        .local_database()?
        .upsert_local_requirement_document(UpsertLocalRequirementDocumentInput {
            document_id: document_id.map(|value| value.trim().to_string()),
            requirement_id,
            owner_user_id: owner.owner_user_id,
            doc_type: optional(request.doc_type)
                .unwrap_or_else(|| "technical_overview".to_string()),
            title: optional(request.title).unwrap_or_else(|| "技术文档".to_string()),
            format: optional(request.format).unwrap_or_else(|| "markdown".to_string()),
            content: required(request.content, "content")?,
        })
        .await?;
    Ok(Json(serde_json::json!(record)))
}
