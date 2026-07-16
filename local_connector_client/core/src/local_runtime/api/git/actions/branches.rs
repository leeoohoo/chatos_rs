// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::extract::State;
use axum::Json;
use serde_json::{json, Value};

use crate::LocalRuntime;

use super::super::super::error::LocalRuntimeApiError;
use super::super::shared::{
    git_command, git_error, resolve_repository, validate_ref, GitCheckoutRequest,
    GitCreateBranchRequest, GitMergeRequest,
};

pub(crate) async fn checkout(
    State(runtime): State<LocalRuntime>,
    Json(request): Json<GitCheckoutRequest>,
) -> Result<Json<Value>, LocalRuntimeApiError> {
    let args = if request.create_tracking.unwrap_or(false) {
        let remote = validate_ref(
            request.remote_branch.as_deref().unwrap_or(""),
            "remote branch",
        )?;
        let local = request
            .branch
            .as_deref()
            .filter(|value| !value.trim().is_empty())
            .map(|value| validate_ref(value, "branch"))
            .transpose()?
            .unwrap_or_else(|| {
                remote
                    .rsplit('/')
                    .next()
                    .unwrap_or(remote.as_str())
                    .to_string()
            });
        vec![
            "checkout".to_string(),
            "-b".to_string(),
            local,
            "--track".to_string(),
            remote,
        ]
    } else {
        vec![
            "checkout".to_string(),
            validate_ref(request.branch.as_deref().unwrap_or(""), "branch")?,
        ]
    };
    execute(&runtime, request.root.as_str(), args, false).await
}

pub(crate) async fn create_branch(
    State(runtime): State<LocalRuntime>,
    Json(request): Json<GitCreateBranchRequest>,
) -> Result<Json<Value>, LocalRuntimeApiError> {
    let name = validate_ref(request.name.as_str(), "branch")?;
    let mut args = if request.checkout.unwrap_or(true) {
        vec!["checkout".to_string(), "-b".to_string(), name]
    } else {
        vec!["branch".to_string(), name]
    };
    if let Some(start) = request
        .start_point
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    {
        args.push(validate_ref(start, "start point")?);
    }
    execute(&runtime, request.root.as_str(), args, false).await
}

pub(crate) async fn merge(
    State(runtime): State<LocalRuntime>,
    Json(request): Json<GitMergeRequest>,
) -> Result<Json<Value>, LocalRuntimeApiError> {
    let branch = validate_ref(request.branch.as_str(), "branch")?;
    let args = match request.mode.as_deref().unwrap_or("default").trim() {
        "" | "default" => vec!["merge", "--no-edit", branch.as_str()],
        "no-ff" => vec!["merge", "--no-ff", "--no-edit", branch.as_str()],
        "ff-only" => vec!["merge", "--ff-only", branch.as_str()],
        _ => return Err(git_error("Unsupported Git merge mode")),
    };
    execute(
        &runtime,
        request.root.as_str(),
        args.into_iter().map(ToOwned::to_owned).collect(),
        true,
    )
    .await
}

async fn execute(
    runtime: &LocalRuntime,
    root: &str,
    args: Vec<String>,
    allow_failure: bool,
) -> Result<Json<Value>, LocalRuntimeApiError> {
    let repo = resolve_repository(runtime, root).await?;
    let output = git_command(repo.as_path(), args, allow_failure)
        .await
        .map_err(git_error)?;
    let summary = super::super::summary_value(runtime, root).await?;
    Ok(Json(json!({
        "success": output.success, "summary": summary,
        "stdout": output.stdout, "stderr": output.stderr,
    })))
}
