// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::fs;
use std::path::{Path, PathBuf};

use axum::extract::State;
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::workspace::paths::relative_to_workspace;
use crate::LocalRuntime;

use super::super::error::LocalRuntimeApiError;
use super::super::workspace_path::resolve_local_workspace_path;

#[derive(Debug, Deserialize)]
pub(super) struct GitignoreRequest {
    path: String,
    mode: String,
}

#[derive(Debug, Deserialize)]
pub(super) struct PathRequest {
    path: String,
}

pub(super) async fn append_gitignore(
    State(runtime): State<LocalRuntime>,
    Json(request): Json<GitignoreRequest>,
) -> Result<Json<Value>, LocalRuntimeApiError> {
    let resolved = resolve_local_workspace_path(&runtime, request.path.as_str(), false).await?;
    let repo_root = find_git_root(
        resolved.path.as_path(),
        resolved.workspace.absolute_root.as_path(),
    )
    .unwrap_or_else(|| resolved.workspace.absolute_root.clone());
    let relative = resolved
        .path
        .strip_prefix(repo_root.as_path())
        .unwrap_or(resolved.path.as_path())
        .to_string_lossy()
        .replace('\\', "/");
    let pattern = match request.mode.trim() {
        "folder" => format!("{}/", relative.trim_end_matches('/')),
        "extension" => resolved
            .path
            .extension()
            .and_then(|value| value.to_str())
            .filter(|value| !value.is_empty())
            .map(|value| format!("*.{value}"))
            .ok_or_else(|| {
                LocalRuntimeApiError::bad_request(
                    "local_runtime_extension_missing",
                    "File extension is missing",
                )
            })?,
        "file" => relative.clone(),
        _ => {
            return Err(LocalRuntimeApiError::bad_request(
                "local_runtime_gitignore_mode_invalid",
                "Unsupported gitignore mode",
            ))
        }
    };
    let gitignore = repo_root.join(".gitignore");
    let existing = fs::read_to_string(gitignore.as_path()).unwrap_or_default();
    let appended = !existing.lines().any(|line| line.trim() == pattern);
    if appended {
        let separator = (!existing.is_empty() && !existing.ends_with('\n'))
            .then_some("\n")
            .unwrap_or("");
        fs::write(
            gitignore.as_path(),
            format!("{existing}{separator}{pattern}\n"),
        )
        .map_err(git_error)?;
    }
    let gitignore_relative = relative_to_workspace(&resolved.workspace, gitignore.as_path());
    Ok(Json(json!({
        "success": true,
        "path": resolved.logical_child(gitignore_relative.as_str()),
        "pattern": pattern,
        "created": existing.is_empty(),
        "appended": appended,
    })))
}

pub(super) async fn discard_git_changes(
    State(runtime): State<LocalRuntime>,
    Json(request): Json<PathRequest>,
) -> Result<Json<Value>, LocalRuntimeApiError> {
    let resolved = resolve_local_workspace_path(&runtime, request.path.as_str(), false).await?;
    let repo_root = find_git_root(
        resolved.path.as_path(),
        resolved.workspace.absolute_root.as_path(),
    )
    .ok_or_else(|| {
        LocalRuntimeApiError::bad_request(
            "local_runtime_git_repo_required",
            "Local path is not inside a Git repository",
        )
    })?;
    let relative = resolved
        .path
        .strip_prefix(repo_root.as_path())
        .map_err(git_error)?;
    let restore = tokio::process::Command::new("git")
        .current_dir(repo_root.as_path())
        .args(["restore", "--staged", "--worktree", "--"])
        .arg(relative)
        .output()
        .await
        .map_err(git_error)?;
    if !restore.status.success() {
        let clean = tokio::process::Command::new("git")
            .current_dir(repo_root.as_path())
            .args(["clean", "-fd", "--"])
            .arg(relative)
            .output()
            .await
            .map_err(git_error)?;
        if !clean.status.success() {
            return Err(git_error(String::from_utf8_lossy(clean.stderr.as_slice())));
        }
    }
    Ok(Json(json!({
        "success": true,
        "path": resolved.logical_path(),
        "stdout": String::from_utf8_lossy(restore.stdout.as_slice()),
        "stderr": String::from_utf8_lossy(restore.stderr.as_slice()),
    })))
}

fn find_git_root(path: &Path, workspace_root: &Path) -> Option<PathBuf> {
    let mut current = if path.is_dir() { path } else { path.parent()? };
    loop {
        if current.join(".git").exists() {
            return Some(current.to_path_buf());
        }
        if current == workspace_root {
            return None;
        }
        current = current.parent()?;
    }
}

fn git_error(error: impl std::fmt::Display) -> LocalRuntimeApiError {
    LocalRuntimeApiError::bad_request("local_runtime_git_failed", error.to_string())
}
