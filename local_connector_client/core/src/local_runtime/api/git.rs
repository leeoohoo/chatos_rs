// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

mod actions;
mod contracts;
mod inspection;
mod shared;

use axum::extract::{Query, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde_json::{json, Value};

use crate::workspace::paths::relative_to_workspace;
use crate::LocalRuntime;

use super::error::LocalRuntimeApiError;
use super::workspace_path::resolve_local_workspace_path;
use shared::{git_text, GitRootQuery};

pub(super) fn router() -> Router<LocalRuntime> {
    Router::new()
        .route("/api/local/runtime/git/client", get(client_info))
        .route("/api/local/runtime/git/summary", get(summary))
        .route("/api/local/runtime/git/branches", get(branches))
        .route("/api/local/runtime/git/status", get(status))
        .route("/api/local/runtime/git/compare", get(inspection::compare))
        .route("/api/local/runtime/git/diff", get(inspection::diff))
        .route("/api/local/runtime/git/fetch", post(actions::fetch))
        .route("/api/local/runtime/git/pull", post(actions::pull))
        .route("/api/local/runtime/git/push", post(actions::push))
        .route("/api/local/runtime/git/checkout", post(actions::checkout))
        .route(
            "/api/local/runtime/git/branch",
            post(actions::create_branch),
        )
        .route("/api/local/runtime/git/merge", post(actions::merge))
        .route("/api/local/runtime/git/stage", post(actions::stage))
        .route("/api/local/runtime/git/unstage", post(actions::unstage))
        .route("/api/local/runtime/git/discard", post(actions::discard))
        .route("/api/local/runtime/git/commit", post(actions::commit))
}

async fn client_info() -> Json<Value> {
    match git_text(std::path::Path::new("."), &["--version"]).await {
        Ok(version) => Json(json!({
            "available": true, "source": "system", "path": "git",
            "version": version.trim(), "error": Value::Null, "bundled_candidates": [],
        })),
        Err(error) => Json(json!({
            "available": false, "source": "system", "path": "git",
            "version": Value::Null, "error": error, "bundled_candidates": [],
        })),
    }
}

async fn summary(
    State(runtime): State<LocalRuntime>,
    Query(query): Query<GitRootQuery>,
) -> Result<Json<Value>, LocalRuntimeApiError> {
    let resolved = resolve_local_workspace_path(&runtime, query.root.as_str(), false).await?;
    let repo_root = match git_text(resolved.path.as_path(), &["rev-parse", "--show-toplevel"]).await
    {
        Ok(value) => std::path::PathBuf::from(value.trim()),
        Err(_) => return Ok(Json(empty_summary(resolved.logical_path().as_str()))),
    };
    let repo_relative = relative_to_workspace(&resolved.workspace, repo_root.as_path());
    let logical_repo_root = resolved.logical_child(repo_relative.as_str());
    let branch = git_text(repo_root.as_path(), &["branch", "--show-current"])
        .await
        .unwrap_or_default()
        .trim()
        .to_string();
    let head = git_text(repo_root.as_path(), &["rev-parse", "--short", "HEAD"])
        .await
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    let upstream = git_text(
        repo_root.as_path(),
        &["rev-parse", "--abbrev-ref", "@{upstream}"],
    )
    .await
    .ok()
    .map(|value| value.trim().to_string())
    .filter(|value| !value.is_empty());
    let status = git_text(repo_root.as_path(), &["status", "--porcelain"])
        .await
        .unwrap_or_default();
    let (ahead, behind) = shared::ahead_behind(repo_root.as_path()).await;
    Ok(Json(json!({
        "is_repo": true, "root": logical_repo_root, "worktree_root": logical_repo_root,
        "query_root": resolved.logical_path(), "resolved_root": logical_repo_root,
        "selected_root": logical_repo_root, "head": head,
        "current_branch": (!branch.is_empty()).then_some(branch.clone()),
        "detached": branch.is_empty(), "upstream": upstream, "ahead": ahead, "behind": behind,
        "dirty": !status.trim().is_empty(), "operation_state": Value::Null,
        "changes": shared::change_counts(status.as_str()),
        "available_repositories": [{
            "root": logical_repo_root,
            "label": repo_root.file_name().and_then(|value| value.to_str()).unwrap_or("repository"),
            "relative_path": repo_relative,
        }],
    })))
}

async fn branches(
    State(runtime): State<LocalRuntime>,
    Query(query): Query<GitRootQuery>,
) -> Result<Json<Value>, LocalRuntimeApiError> {
    let resolved = resolve_local_workspace_path(&runtime, query.root.as_str(), false).await?;
    let current = git_text(resolved.path.as_path(), &["branch", "--show-current"])
        .await
        .unwrap_or_default()
        .trim()
        .to_string();
    let locals = git_text(
        resolved.path.as_path(),
        &["for-each-ref", "--format=%(refname:short)", "refs/heads"],
    )
    .await
    .unwrap_or_default()
    .lines()
    .filter(|name| !name.trim().is_empty())
    .map(|name| shared::branch_value(name, name == current))
    .collect::<Vec<_>>();
    let remotes = git_text(
        resolved.path.as_path(),
        &["for-each-ref", "--format=%(refname:short)", "refs/remotes"],
    )
    .await
    .unwrap_or_default()
    .lines()
    .filter(|name| !name.trim().is_empty() && !name.ends_with("/HEAD"))
    .map(|name| shared::branch_value(name, false))
    .collect::<Vec<_>>();
    Ok(Json(json!({
        "current": (!current.is_empty()).then_some(current), "locals": locals, "remotes": remotes,
    })))
}

async fn status(
    State(runtime): State<LocalRuntime>,
    Query(query): Query<GitRootQuery>,
) -> Result<Json<Value>, LocalRuntimeApiError> {
    let resolved = resolve_local_workspace_path(&runtime, query.root.as_str(), false).await?;
    let raw = git_text(resolved.path.as_path(), &["status", "--porcelain"])
        .await
        .map_err(shared::git_error)?;
    let files = raw
        .lines()
        .filter_map(shared::status_file)
        .collect::<Vec<_>>();
    Ok(Json(json!({ "files": files })))
}

pub(super) async fn summary_value(
    runtime: &LocalRuntime,
    root: &str,
) -> Result<Value, LocalRuntimeApiError> {
    summary(
        State(runtime.clone()),
        Query(GitRootQuery {
            root: root.to_string(),
        }),
    )
    .await
    .map(|Json(value)| value)
}

fn empty_summary(query_root: &str) -> Value {
    json!({
        "is_repo": false, "root": Value::Null, "worktree_root": Value::Null,
        "query_root": query_root, "resolved_root": Value::Null, "selected_root": Value::Null,
        "head": Value::Null, "current_branch": Value::Null, "detached": false,
        "upstream": Value::Null, "ahead": 0, "behind": 0, "dirty": false,
        "operation_state": Value::Null,
        "changes": { "staged": 0, "unstaged": 0, "untracked": 0, "conflicted": 0 },
        "available_repositories": [],
    })
}
