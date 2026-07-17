// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::extract::State;
use axum::Json;
use serde_json::{json, Value};

use crate::LocalRuntime;

use super::super::super::error::LocalRuntimeApiError;
use super::super::shared::{
    git_command, git_error, git_text, resolve_repository, validate_paths, GitCommandOutput,
    GitCommitRequest, GitPathRequest,
};

pub(crate) async fn stage(
    State(runtime): State<LocalRuntime>,
    Json(request): Json<GitPathRequest>,
) -> Result<Json<Value>, LocalRuntimeApiError> {
    path_action(&runtime, request.root.as_str(), &["add"], &request.paths).await
}

pub(crate) async fn unstage(
    State(runtime): State<LocalRuntime>,
    Json(request): Json<GitPathRequest>,
) -> Result<Json<Value>, LocalRuntimeApiError> {
    path_action(
        &runtime,
        request.root.as_str(),
        &["restore", "--staged"],
        &request.paths,
    )
    .await
}

pub(crate) async fn discard(
    State(runtime): State<LocalRuntime>,
    Json(request): Json<GitPathRequest>,
) -> Result<Json<Value>, LocalRuntimeApiError> {
    let repo = resolve_repository(&runtime, request.root.as_str()).await?;
    let paths = validate_paths(&request.paths)?;
    let mut tracked = Vec::new();
    let mut untracked = Vec::new();
    for path in paths {
        if git_text(
            repo.as_path(),
            &["ls-files", "--error-unmatch", "--", path.as_str()],
        )
        .await
        .is_ok()
        {
            tracked.push(path);
        } else {
            untracked.push(path);
        }
    }
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    if !tracked.is_empty() {
        collect_output(
            &mut stdout,
            &mut stderr,
            run_paths(
                repo.as_path(),
                &["restore", "--staged", "--worktree"],
                tracked,
            )
            .await?,
        );
    }
    if !untracked.is_empty() {
        collect_output(
            &mut stdout,
            &mut stderr,
            run_paths(repo.as_path(), &["clean", "-f"], untracked).await?,
        );
    }
    action_response(
        &runtime,
        request.root.as_str(),
        stdout.join("\n"),
        stderr.join("\n"),
    )
    .await
}

pub(crate) async fn commit(
    State(runtime): State<LocalRuntime>,
    Json(request): Json<GitCommitRequest>,
) -> Result<Json<Value>, LocalRuntimeApiError> {
    let repo = resolve_repository(&runtime, request.root.as_str()).await?;
    let message = request.message.trim();
    if message.is_empty() {
        return Err(git_error("Git commit message is required"));
    }
    if let Some(paths) = request.paths.as_ref().filter(|paths| !paths.is_empty()) {
        run_paths(repo.as_path(), &["add"], validate_paths(paths)?).await?;
    }
    let output = git_command(
        repo.as_path(),
        vec!["commit".to_string(), "-m".to_string(), message.to_string()],
        false,
    )
    .await
    .map_err(git_error)?;
    action_response(
        &runtime,
        request.root.as_str(),
        output.stdout,
        output.stderr,
    )
    .await
}

async fn path_action(
    runtime: &LocalRuntime,
    root: &str,
    prefix: &[&str],
    paths: &[String],
) -> Result<Json<Value>, LocalRuntimeApiError> {
    let repo = resolve_repository(runtime, root).await?;
    let output = run_paths(repo.as_path(), prefix, validate_paths(paths)?).await?;
    action_response(runtime, root, output.stdout, output.stderr).await
}

async fn run_paths(
    repo: &std::path::Path,
    prefix: &[&str],
    paths: Vec<String>,
) -> Result<GitCommandOutput, LocalRuntimeApiError> {
    let mut args = prefix
        .iter()
        .map(|value| value.to_string())
        .collect::<Vec<_>>();
    args.push("--".to_string());
    args.extend(paths);
    git_command(repo, args, false).await.map_err(git_error)
}

async fn action_response(
    runtime: &LocalRuntime,
    root: &str,
    stdout: String,
    stderr: String,
) -> Result<Json<Value>, LocalRuntimeApiError> {
    let summary = super::super::summary_value(runtime, root).await?;
    Ok(Json(json!({
        "success": true, "summary": summary, "stdout": stdout, "stderr": stderr,
    })))
}

fn collect_output(stdout: &mut Vec<String>, stderr: &mut Vec<String>, output: GitCommandOutput) {
    if !output.stdout.trim().is_empty() {
        stdout.push(output.stdout.trim().to_string());
    }
    if !output.stderr.trim().is_empty() {
        stderr.push(output.stderr.trim().to_string());
    }
}
