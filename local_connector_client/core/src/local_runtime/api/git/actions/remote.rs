// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::extract::State;
use axum::Json;
use serde_json::{json, Value};

use crate::LocalRuntime;

use super::super::super::error::LocalRuntimeApiError;
use super::super::shared::{
    git_command, git_error, git_text, resolve_repository, validate_ref, GitFetchRequest,
    GitPullRequest, GitPushRequest,
};

pub(crate) async fn fetch(
    State(runtime): State<LocalRuntime>,
    Json(request): Json<GitFetchRequest>,
) -> Result<Json<Value>, LocalRuntimeApiError> {
    let remote = validate_ref(request.remote.as_deref().unwrap_or("origin"), "remote")?;
    execute(
        &runtime,
        request.root.as_str(),
        vec!["fetch".to_string(), "--prune".to_string(), remote],
    )
    .await
}

pub(crate) async fn pull(
    State(runtime): State<LocalRuntime>,
    Json(request): Json<GitPullRequest>,
) -> Result<Json<Value>, LocalRuntimeApiError> {
    let args = match request.mode.as_deref().unwrap_or("ff-only").trim() {
        "" | "ff-only" => vec!["pull".to_string(), "--ff-only".to_string()],
        "rebase" => vec!["pull".to_string(), "--rebase".to_string()],
        _ => return Err(git_error("Unsupported Git pull mode")),
    };
    execute(&runtime, request.root.as_str(), args).await
}

pub(crate) async fn push(
    State(runtime): State<LocalRuntime>,
    Json(request): Json<GitPushRequest>,
) -> Result<Json<Value>, LocalRuntimeApiError> {
    let repo = resolve_repository(&runtime, request.root.as_str()).await?;
    let remote = validate_ref(request.remote.as_deref().unwrap_or("origin"), "remote")?;
    let branch = match request
        .branch
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    {
        Some(value) => validate_ref(value, "branch")?,
        None => validate_ref(
            git_text(repo.as_path(), &["branch", "--show-current"])
                .await
                .map_err(git_error)?
                .as_str(),
            "branch",
        )?,
    };
    let mut args = vec!["push".to_string()];
    if request.set_upstream.unwrap_or(false) {
        args.push("-u".to_string());
    }
    args.extend([remote, branch]);
    execute_resolved(&runtime, request.root.as_str(), repo, args).await
}

async fn execute(
    runtime: &LocalRuntime,
    root: &str,
    args: Vec<String>,
) -> Result<Json<Value>, LocalRuntimeApiError> {
    let repo = resolve_repository(runtime, root).await?;
    execute_resolved(runtime, root, repo, args).await
}

async fn execute_resolved(
    runtime: &LocalRuntime,
    root: &str,
    repo: std::path::PathBuf,
    args: Vec<String>,
) -> Result<Json<Value>, LocalRuntimeApiError> {
    let output = git_command(repo.as_path(), args, false)
        .await
        .map_err(git_error)?;
    let summary = super::super::summary_value(runtime, root).await?;
    Ok(Json(json!({
        "success": true, "summary": summary,
        "stdout": output.stdout, "stderr": output.stderr,
    })))
}
