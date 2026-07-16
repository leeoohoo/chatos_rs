// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::fs;
use std::path::{Component, Path};

use axum::extract::State;
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::workspace::paths::relative_to_workspace;
use crate::LocalRuntime;

use super::super::error::LocalRuntimeApiError;
use super::super::workspace_path::resolve_local_workspace_path;

#[derive(Debug, Deserialize)]
pub(super) struct ParentNameRequest {
    parent_path: String,
    name: String,
    content: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(super) struct WriteRequest {
    path: String,
    content: String,
}

#[derive(Debug, Deserialize)]
pub(super) struct DeleteRequest {
    path: String,
    recursive: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub(super) struct MoveRequest {
    source_path: String,
    target_parent_path: String,
    target_name: Option<String>,
    replace_existing: Option<bool>,
}

pub(super) async fn create_directory(
    State(runtime): State<LocalRuntime>,
    Json(request): Json<ParentNameRequest>,
) -> Result<Json<Value>, LocalRuntimeApiError> {
    let parent =
        resolve_local_workspace_path(&runtime, request.parent_path.as_str(), false).await?;
    let name = valid_name(request.name.as_str())?;
    let path = parent.path.join(name.as_str());
    ensure_missing_child(&parent.path, path.as_path())?;
    fs::create_dir(path.as_path()).map_err(fs_error)?;
    mutation_response(&parent, path.as_path(), name.as_str(), true)
}

pub(super) async fn create_file(
    State(runtime): State<LocalRuntime>,
    Json(request): Json<ParentNameRequest>,
) -> Result<Json<Value>, LocalRuntimeApiError> {
    let parent =
        resolve_local_workspace_path(&runtime, request.parent_path.as_str(), false).await?;
    let name = valid_name(request.name.as_str())?;
    let path = parent.path.join(name.as_str());
    ensure_missing_child(&parent.path, path.as_path())?;
    fs::write(path.as_path(), request.content.unwrap_or_default()).map_err(fs_error)?;
    mutation_response(&parent, path.as_path(), name.as_str(), true)
}

pub(super) async fn write_file(
    State(runtime): State<LocalRuntime>,
    Json(request): Json<WriteRequest>,
) -> Result<Json<Value>, LocalRuntimeApiError> {
    let resolved = resolve_local_workspace_path(&runtime, request.path.as_str(), false).await?;
    if !resolved.path.is_file() {
        return Err(LocalRuntimeApiError::bad_request(
            "local_runtime_file_required",
            "Local path is not a file",
        ));
    }
    fs::write(resolved.path.as_path(), request.content).map_err(fs_error)?;
    Ok(Json(json!({
        "success": true,
        "path": resolved.logical_path(),
        "name": resolved.path.file_name().and_then(|value| value.to_str()).unwrap_or(""),
    })))
}

pub(super) async fn delete_entry(
    State(runtime): State<LocalRuntime>,
    Json(request): Json<DeleteRequest>,
) -> Result<Json<Value>, LocalRuntimeApiError> {
    let resolved = resolve_local_workspace_path(&runtime, request.path.as_str(), false).await?;
    forbid_workspace_root(&resolved.path, resolved.workspace.absolute_root.as_path())?;
    if resolved.path.is_dir() {
        if request.recursive.unwrap_or(false) {
            fs::remove_dir_all(resolved.path.as_path()).map_err(fs_error)?;
        } else {
            fs::remove_dir(resolved.path.as_path()).map_err(fs_error)?;
        }
    } else {
        fs::remove_file(resolved.path.as_path()).map_err(fs_error)?;
    }
    Ok(Json(
        json!({ "success": true, "path": resolved.logical_path() }),
    ))
}

pub(super) async fn move_entry(
    State(runtime): State<LocalRuntime>,
    Json(request): Json<MoveRequest>,
) -> Result<Json<Value>, LocalRuntimeApiError> {
    let source =
        resolve_local_workspace_path(&runtime, request.source_path.as_str(), false).await?;
    let target_parent =
        resolve_local_workspace_path(&runtime, request.target_parent_path.as_str(), false).await?;
    if source.workspace.id != target_parent.workspace.id {
        return Err(LocalRuntimeApiError::bad_request(
            "local_runtime_cross_workspace_move",
            "Moving entries across local workspaces is not supported",
        ));
    }
    forbid_workspace_root(&source.path, source.workspace.absolute_root.as_path())?;
    let fallback_name = source
        .path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("");
    let target_name = valid_name(request.target_name.as_deref().unwrap_or(fallback_name))?;
    let target = target_parent.path.join(target_name.as_str());
    if target.exists() {
        if !request.replace_existing.unwrap_or(false) {
            return Err(LocalRuntimeApiError::conflict(
                "local_runtime_target_exists",
                "The target path already exists",
            ));
        }
        if target.is_dir() {
            fs::remove_dir_all(target.as_path()).map_err(fs_error)?;
        } else {
            fs::remove_file(target.as_path()).map_err(fs_error)?;
        }
    }
    fs::rename(source.path.as_path(), target.as_path()).map_err(fs_error)?;
    let relative = relative_to_workspace(&target_parent.workspace, target.as_path());
    Ok(Json(json!({
        "success": true,
        "path": source.logical_path(),
        "to_path": target_parent.logical_child(relative.as_str()),
        "name": target_name,
    })))
}

fn mutation_response(
    parent: &super::super::workspace_path::LocalWorkspacePath,
    path: &Path,
    name: &str,
    created: bool,
) -> Result<Json<Value>, LocalRuntimeApiError> {
    let relative = relative_to_workspace(&parent.workspace, path);
    Ok(Json(json!({
        "success": true,
        "path": parent.logical_child(relative.as_str()),
        "name": name,
        "created": created,
    })))
}

fn valid_name(value: &str) -> Result<String, LocalRuntimeApiError> {
    let value = value.trim();
    let components = Path::new(value).components().collect::<Vec<_>>();
    if value.is_empty() || components.len() != 1 || !matches!(components[0], Component::Normal(_)) {
        return Err(LocalRuntimeApiError::bad_request(
            "local_runtime_name_invalid",
            "File or directory name is invalid",
        ));
    }
    Ok(value.to_string())
}

fn ensure_missing_child(parent: &Path, child: &Path) -> Result<(), LocalRuntimeApiError> {
    if !parent.is_dir() {
        return Err(LocalRuntimeApiError::bad_request(
            "local_runtime_directory_required",
            "Parent path is not a directory",
        ));
    }
    if child.exists() {
        return Err(LocalRuntimeApiError::conflict(
            "local_runtime_target_exists",
            "The target path already exists",
        ));
    }
    Ok(())
}

fn forbid_workspace_root(path: &Path, workspace_root: &Path) -> Result<(), LocalRuntimeApiError> {
    if path == workspace_root
        || path.canonicalize().ok().as_deref() == workspace_root.canonicalize().ok().as_deref()
    {
        return Err(LocalRuntimeApiError::bad_request(
            "local_runtime_root_mutation_forbidden",
            "The workspace root cannot be modified",
        ));
    }
    Ok(())
}

fn fs_error(error: impl std::fmt::Display) -> LocalRuntimeApiError {
    LocalRuntimeApiError::bad_request("local_runtime_fs_mutation_failed", error.to_string())
}
