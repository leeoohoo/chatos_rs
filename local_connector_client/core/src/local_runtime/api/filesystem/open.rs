// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::path::Path;
use std::process::Stdio;

use axum::extract::State;
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::LocalRuntime;

use super::super::error::LocalRuntimeApiError;
use super::super::workspace_path::resolve_local_workspace_path;

#[derive(Debug, Deserialize)]
pub(super) struct OpenRequest {
    path: String,
    mode: Option<String>,
}

pub(super) async fn open_path(
    State(runtime): State<LocalRuntime>,
    Json(request): Json<OpenRequest>,
) -> Result<Json<Value>, LocalRuntimeApiError> {
    let resolved = resolve_local_workspace_path(&runtime, request.path.as_str(), false).await?;
    let mode = request.mode.as_deref().unwrap_or("default").trim();
    let mut command = external_open_command(resolved.path.as_path(), mode)?;
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    command.spawn().map_err(|error| {
        LocalRuntimeApiError::bad_request("local_runtime_open_failed", error.to_string())
    })?;
    Ok(Json(
        json!({ "success": true, "path": resolved.logical_path(), "mode": mode }),
    ))
}

fn external_open_command(
    path: &Path,
    mode: &str,
) -> Result<tokio::process::Command, LocalRuntimeApiError> {
    if mode == "code" {
        let mut command = tokio::process::Command::new("code");
        command.arg(path);
        return Ok(command);
    }
    let mut command = match std::env::consts::OS {
        "macos" => {
            let mut command = tokio::process::Command::new("open");
            if mode == "reveal" {
                command.arg("-R");
            }
            command
        }
        "windows" => {
            let mut command = tokio::process::Command::new("explorer");
            if mode == "reveal" {
                command.arg("/select,");
            }
            command
        }
        _ => tokio::process::Command::new("xdg-open"),
    };
    if !matches!(mode, "default" | "reveal") {
        return Err(LocalRuntimeApiError::bad_request(
            "local_runtime_open_mode_invalid",
            "Unsupported open mode",
        ));
    }
    command.arg(path);
    Ok(command)
}
