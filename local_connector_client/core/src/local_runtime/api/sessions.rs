// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::extract::{Path, Query, State};
use axum::Json;
use serde::Deserialize;

use crate::local_runtime::storage::{
    CreateLocalSessionInput, LocalSessionRecord, UpsertLocalProjectInput,
};
use crate::local_runtime::{
    LOCAL_UNSCOPED_PROJECT_ID, LOCAL_UNSCOPED_PROJECT_NAME, LOCAL_UNSCOPED_WORKSPACE_ID,
};
use crate::LocalRuntime;

use super::context::owner_context;
use super::error::LocalRuntimeApiError;

#[derive(Debug, Deserialize)]
pub(super) struct LocalSessionListQuery {
    project_id: String,
}

#[derive(Debug, Deserialize)]
pub(super) struct CreateLocalSessionRequest {
    project_id: String,
    title: Option<String>,
    contact_id: Option<String>,
    selected_model_id: Option<String>,
    selected_agent_id: Option<String>,
}

pub(super) async fn list_sessions(
    State(runtime): State<LocalRuntime>,
    Query(query): Query<LocalSessionListQuery>,
) -> Result<Json<Vec<LocalSessionRecord>>, LocalRuntimeApiError> {
    let owner = owner_context(&runtime).await?;
    let project_id = required(query.project_id, "project_id")?;
    runtime
        .local_database()?
        .list_sessions(owner.owner_user_id.as_str(), project_id.as_str())
        .await
        .map(Json)
        .map_err(LocalRuntimeApiError::from)
}

pub(super) async fn create_session(
    State(runtime): State<LocalRuntime>,
    Json(request): Json<CreateLocalSessionRequest>,
) -> Result<Json<LocalSessionRecord>, LocalRuntimeApiError> {
    let owner = owner_context(&runtime).await?;
    let project_id = required(request.project_id, "project_id")?;
    let title = request
        .title
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "Untitled".to_string());

    let contact_id = normalize_optional(request.contact_id);
    if project_id == LOCAL_UNSCOPED_PROJECT_ID {
        runtime
            .local_database()?
            .upsert_project(UpsertLocalProjectInput {
                project_id: LOCAL_UNSCOPED_PROJECT_ID.to_string(),
                owner_user_id: owner.owner_user_id.clone(),
                device_id: owner.device_id.clone(),
                workspace_id: LOCAL_UNSCOPED_WORKSPACE_ID.to_string(),
                project_name: LOCAL_UNSCOPED_PROJECT_NAME.to_string(),
                root_relative_path: None,
            })
            .await
            .map_err(LocalRuntimeApiError::from)?;
    }
    runtime
        .local_database()?
        .create_session_with_contact(
            CreateLocalSessionInput {
                project_id,
                owner_user_id: owner.owner_user_id,
                title,
                selected_model_id: normalize_optional(request.selected_model_id),
                selected_agent_id: normalize_optional(request.selected_agent_id),
            },
            contact_id,
        )
        .await
        .map(Json)
        .map_err(LocalRuntimeApiError::from)
}

pub(super) async fn get_session(
    Path(session_id): Path<String>,
    State(runtime): State<LocalRuntime>,
) -> Result<Json<LocalSessionRecord>, LocalRuntimeApiError> {
    let owner = owner_context(&runtime).await?;
    let session_id = required(session_id, "session_id")?;
    runtime
        .local_database()?
        .get_session(session_id.as_str(), owner.owner_user_id.as_str())
        .await
        .map_err(LocalRuntimeApiError::from)?
        .map(Json)
        .ok_or_else(|| {
            LocalRuntimeApiError::not_found(
                "local_runtime_session_not_found",
                "Local runtime session was not found",
            )
        })
}

fn required(value: String, field: &'static str) -> Result<String, LocalRuntimeApiError> {
    let value = value.trim().to_string();
    if value.is_empty() {
        return Err(LocalRuntimeApiError::bad_request(
            "local_runtime_invalid_request",
            format!("{field} is required"),
        ));
    }
    Ok(value)
}

fn normalize_optional(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}
