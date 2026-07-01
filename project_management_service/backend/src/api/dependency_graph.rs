// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::extract::{Path, Query, State};
use axum::{Extension, Json};
use serde::Deserialize;

use super::access::{require_project_access, require_requirement_access, require_work_item_access};
use super::ApiError;
use crate::auth::CurrentUser;
use crate::models::DependencyGraphResponse;
use crate::services::dependency_graph;
use crate::state::AppState;

pub(in crate::api) async fn get_requirement_dependency_graph(
    Path(requirement_id): Path<String>,
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
) -> Result<Json<DependencyGraphResponse>, ApiError> {
    let requirement = require_requirement_access(&state, &requirement_id, &user).await?;
    dependency_graph::requirement_dependency_graph(&state.store, &requirement)
        .await
        .map(Json)
        .map_err(ApiError::bad_request)
}

pub(in crate::api) async fn get_work_item_dependency_graph(
    Path(work_item_id): Path<String>,
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
) -> Result<Json<DependencyGraphResponse>, ApiError> {
    let item = require_work_item_access(&state, &work_item_id, &user).await?;
    dependency_graph::work_item_dependency_graph(&state.store, &item)
        .await
        .map(Json)
        .map_err(ApiError::bad_request)
}

#[derive(Debug, Default, Deserialize)]
pub(in crate::api) struct DependencyGraphQuery {
    include_archived: Option<bool>,
}

pub(in crate::api) async fn get_project_dependency_graph(
    Path(project_id): Path<String>,
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Query(query): Query<DependencyGraphQuery>,
) -> Result<Json<DependencyGraphResponse>, ApiError> {
    require_project_access(&state, &project_id, &user).await?;
    let include_archived = query.include_archived.unwrap_or(false);
    dependency_graph::project_dependency_graph(&state.store, &project_id, include_archived)
        .await
        .map(Json)
        .map_err(ApiError::bad_request)
}
