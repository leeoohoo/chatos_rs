// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::extract::{Path, State};
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::LocalRuntime;

use super::super::context::owner_context;
use super::super::error::LocalRuntimeApiError;
use super::{analysis, project};

#[derive(Debug, Default, Deserialize)]
pub(super) struct RunEnvironmentUpdate {
    #[serde(default)]
    selected_toolchains: Value,
    #[serde(default)]
    custom_toolchains: Value,
    #[serde(default)]
    env_vars: Value,
    terminal_ui_enabled: Option<bool>,
}

pub(super) async fn get(
    Path(project_id): Path<String>,
    State(runtime): State<LocalRuntime>,
) -> Result<Json<Value>, LocalRuntimeApiError> {
    respond(
        &runtime,
        project_id.as_str(),
        &RunEnvironmentUpdate::default(),
    )
    .await
}

pub(super) async fn update(
    Path(project_id): Path<String>,
    State(runtime): State<LocalRuntime>,
    Json(request): Json<RunEnvironmentUpdate>,
) -> Result<Json<Value>, LocalRuntimeApiError> {
    respond(&runtime, project_id.as_str(), &request).await
}

async fn respond(
    runtime: &LocalRuntime,
    project_id: &str,
    request: &RunEnvironmentUpdate,
) -> Result<Json<Value>, LocalRuntimeApiError> {
    let owner = owner_context(runtime).await?;
    let (root, logical_root) = project::project_root(runtime, project_id).await?;
    let analysis = analysis::analyze_project(root.as_path(), logical_root.as_str());
    let config_files = analysis
        .get("config_files")
        .cloned()
        .unwrap_or_else(|| json!([]));
    Ok(Json(environment_response(
        project_id,
        owner.owner_user_id.as_str(),
        request,
        config_files,
    )))
}

fn environment_response(
    project_id: &str,
    owner_user_id: &str,
    request: &RunEnvironmentUpdate,
    config_files: Value,
) -> Value {
    json!({
        "project_id": project_id,
        "user_id": owner_user_id,
        "options_by_kind": {},
        "config_files": config_files,
        "validation_issues": [],
        "selected_toolchains": request.selected_toolchains,
        "custom_toolchains": request.custom_toolchains,
        "env_vars": request.env_vars,
        "terminal_ui_enabled": request.terminal_ui_enabled.unwrap_or(false),
        "updated_at": crate::local_now_rfc3339(),
    })
}
