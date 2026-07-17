// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::process::Stdio;

use axum::extract::{Path, State};
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::LocalRuntime;

use super::super::error::LocalRuntimeApiError;
use super::{analysis, project};

#[derive(Debug, Default, Deserialize)]
pub(super) struct SetDefaultRequest {
    target_id: String,
}

#[derive(Debug, Default, Deserialize)]
pub(super) struct ExecuteRequest {
    target_id: Option<String>,
    command: Option<String>,
    cwd: Option<String>,
}

pub(super) async fn state(Path(project_id): Path<String>) -> Json<Value> {
    Json(json!({
        "project_id": project_id,
        "running": false,
        "busy": false,
        "status": "idle",
        "terminal_id": Value::Null,
        "terminal_name": Value::Null,
        "cwd": Value::Null,
        "terminal": Value::Null,
        "instances": [],
    }))
}

pub(super) async fn set_default(
    Path(project_id): Path<String>,
    State(runtime): State<LocalRuntime>,
    Json(request): Json<SetDefaultRequest>,
) -> Result<Json<Value>, LocalRuntimeApiError> {
    let mut catalog = project::catalog_for_project(&runtime, project_id.as_str()).await?;
    let target_exists = catalog
        .get("targets")
        .and_then(Value::as_array)
        .is_some_and(|targets| {
            targets.iter().any(|target| {
                target.get("id").and_then(Value::as_str) == Some(request.target_id.as_str())
            })
        });
    if !target_exists {
        return Err(LocalRuntimeApiError::not_found(
            "local_runtime_run_target_not_found",
            "Local run target was not found",
        ));
    }
    catalog["default_target_id"] = Value::String(request.target_id);
    Ok(Json(catalog))
}

pub(super) async fn execute(
    Path(project_id): Path<String>,
    State(runtime): State<LocalRuntime>,
    Json(request): Json<ExecuteRequest>,
) -> Result<Json<Value>, LocalRuntimeApiError> {
    let (root, logical_root) = project::project_root(&runtime, project_id.as_str()).await?;
    let catalog = analysis::analyze_project(root.as_path(), logical_root.as_str());
    let target = request.target_id.as_deref().and_then(|target_id| {
        catalog
            .get("targets")
            .and_then(Value::as_array)?
            .iter()
            .find(|target| target.get("id").and_then(Value::as_str) == Some(target_id))
    });
    let command = request
        .command
        .or_else(|| {
            target.and_then(|target| {
                target
                    .get("command")
                    .and_then(Value::as_str)
                    .map(ToOwned::to_owned)
            })
        })
        .ok_or_else(|| {
            LocalRuntimeApiError::bad_request(
                "local_runtime_run_command_required",
                "Local run command is required",
            )
        })?;
    let cwd = request.cwd.unwrap_or(logical_root);
    let shell = crate::select_local_shell();
    let mut child = tokio::process::Command::new(shell);
    child.current_dir(root.as_path());
    if cfg!(windows) {
        child.args(["/C", command.as_str()]);
    } else {
        child.args(["-lc", command.as_str()]);
    }
    child
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    child.spawn().map_err(|error| {
        LocalRuntimeApiError::bad_request("local_runtime_run_failed", error.to_string())
    })?;
    Ok(Json(json!({
        "success": true,
        "status": "running",
        "run_id": format!("local_run_{}", uuid::Uuid::new_v4()),
        "target_id": request.target_id,
        "command": command,
        "cwd": cwd,
    })))
}
