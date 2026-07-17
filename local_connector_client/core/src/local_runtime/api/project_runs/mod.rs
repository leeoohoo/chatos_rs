// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

mod analysis;
mod environment;
mod execution;
mod language_targets;
mod project;

use axum::extract::{Path, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde_json::Value;

use crate::LocalRuntime;

use super::error::LocalRuntimeApiError;

pub(super) fn router() -> Router<LocalRuntime> {
    Router::new()
        .route(
            "/api/local/runtime/projects/{project_id}/run/analyze",
            post(analyze),
        )
        .route(
            "/api/local/runtime/projects/{project_id}/run/catalog",
            get(catalog),
        )
        .route(
            "/api/local/runtime/projects/{project_id}/run/state",
            get(execution::state),
        )
        .route(
            "/api/local/runtime/projects/{project_id}/run/default",
            post(execution::set_default),
        )
        .route(
            "/api/local/runtime/projects/{project_id}/run/environment",
            get(environment::get).put(environment::update),
        )
        .route(
            "/api/local/runtime/projects/{project_id}/run/execute",
            post(execution::execute),
        )
}

async fn analyze(
    Path(project_id): Path<String>,
    State(runtime): State<LocalRuntime>,
) -> Result<Json<Value>, LocalRuntimeApiError> {
    project::catalog_for_project(&runtime, project_id.as_str())
        .await
        .map(Json)
}

async fn catalog(
    Path(project_id): Path<String>,
    State(runtime): State<LocalRuntime>,
) -> Result<Json<Value>, LocalRuntimeApiError> {
    project::catalog_for_project(&runtime, project_id.as_str())
        .await
        .map(Json)
}
