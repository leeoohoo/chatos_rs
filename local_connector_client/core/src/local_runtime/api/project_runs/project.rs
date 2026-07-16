// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde_json::Value;

use crate::workspace::paths::canonicalize_existing_dir;
use crate::LocalRuntime;

use super::super::context::owner_context;
use super::super::error::LocalRuntimeApiError;
use super::super::workspace_path::logical_workspace_path;
use super::analysis;

pub(super) async fn catalog_for_project(
    runtime: &LocalRuntime,
    project_id: &str,
) -> Result<Value, LocalRuntimeApiError> {
    let (root, logical_root) = project_root(runtime, project_id).await?;
    let project_id = project_id.to_string();
    let mut result = tokio::task::spawn_blocking(move || {
        analysis::analyze_project(root.as_path(), logical_root.as_str())
    })
    .await
    .map_err(|error| LocalRuntimeApiError::internal(error.to_string()))?;
    result["project_id"] = Value::String(project_id);
    Ok(result)
}

pub(super) async fn project_root(
    runtime: &LocalRuntime,
    project_id: &str,
) -> Result<(std::path::PathBuf, String), LocalRuntimeApiError> {
    let owner = owner_context(runtime).await?;
    let project = ensure_project(runtime, owner.owner_user_id.as_str(), project_id).await?;
    let state = runtime.state.read().await;
    let workspace = state
        .workspace_by_id(project.workspace_id.as_str())
        .ok_or_else(|| {
            LocalRuntimeApiError::not_found(
                "local_runtime_workspace_not_found",
                "The local project workspace is not registered",
            )
        })?;
    let workspace_root = canonicalize_existing_dir(workspace.absolute_root.as_path())
        .map_err(LocalRuntimeApiError::from)?;
    let relative = project.root_relative_path.as_deref().unwrap_or(".");
    let root = if relative == "." {
        workspace_root
    } else {
        workspace_root
            .join(relative)
            .canonicalize()
            .map_err(|error| {
                LocalRuntimeApiError::bad_request(
                    "local_runtime_project_root_invalid",
                    error.to_string(),
                )
            })?
    };
    if !root.starts_with(workspace.absolute_root.as_path()) || !root.is_dir() {
        return Err(LocalRuntimeApiError::bad_request(
            "local_runtime_project_root_invalid",
            "Local project root is invalid",
        ));
    }
    Ok((
        root,
        logical_workspace_path(owner.device_id.as_str(), workspace.id.as_str(), relative),
    ))
}

async fn ensure_project(
    runtime: &LocalRuntime,
    owner_user_id: &str,
    project_id: &str,
) -> Result<crate::local_runtime::storage::LocalProjectRecord, LocalRuntimeApiError> {
    runtime
        .local_database()?
        .get_project(project_id, owner_user_id)
        .await?
        .ok_or_else(|| {
            LocalRuntimeApiError::not_found(
                "local_runtime_project_not_found",
                "Local project was not found",
            )
        })
}
