// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::extract::Query;
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde_json::{json, Value};
use std::path::Path;

use crate::api::fs::policy::{FsPathPolicy, FsPolicyError};
use crate::api::local_connectors::parse_local_connector_root_path;
use crate::config::Config;
use crate::core::auth::AuthUser;
use crate::core::project_access::{ensure_owned_project, map_project_access_error};
use crate::models::project::harness_project_id_from_root_path;
use crate::services::git;
use crate::services::git::{
    GitActionResult, GitCheckoutRequest, GitCommitRequest, GitCompareQuery, GitCreateBranchRequest,
    GitDiffQuery, GitFetchRequest, GitMergeRequest, GitPathRequest, GitPullRequest, GitPushRequest,
    GitRepositoryCandidate, GitRootQuery, GitSummary,
};
use crate::services::project_management_api_client;

pub fn router() -> Router {
    Router::new()
        .route("/api/git/client", get(client))
        .route("/api/git/summary", get(summary))
        .route("/api/git/branches", get(branches))
        .route("/api/git/status", get(status))
        .route("/api/git/compare", get(compare))
        .route("/api/git/diff", get(diff))
        .route("/api/git/fetch", post(fetch))
        .route("/api/git/pull", post(pull))
        .route("/api/git/push", post(push))
        .route("/api/git/checkout", post(checkout))
        .route("/api/git/branch", post(create_branch))
        .route("/api/git/merge", post(merge_branch))
        .route("/api/git/stage", post(stage))
        .route("/api/git/unstage", post(unstage))
        .route("/api/git/discard", post(discard))
        .route("/api/git/commit", post(commit))
}

async fn client() -> (StatusCode, Json<Value>) {
    (StatusCode::OK, Json(json!(git::client_info().await)))
}

async fn summary(auth: AuthUser, Query(query): Query<GitRootQuery>) -> (StatusCode, Json<Value>) {
    if let Some(project_id) = harness_project_id_from_root_path(query.root.as_str()) {
        return harness_git_summary(&auth, project_id, query.root.as_str()).await;
    }
    let policy = match git_path_policy(&auth).await {
        Ok(policy) => policy,
        Err(err) => return err,
    };
    let root = match authorize_git_root(&policy, query.root.as_str(), false) {
        Ok(root) => root,
        Err(err) => return err,
    };
    let preferred_repo_root =
        match authorize_optional_git_root(&policy, query.preferred_repo_root.as_deref(), false) {
            Ok(root) => root,
            Err(err) => return err,
        };
    match git::summary(
        root.as_str(),
        preferred_repo_root.as_deref(),
        query.force_refresh.unwrap_or(false),
    )
    .await
    {
        Ok(response) => (
            StatusCode::OK,
            Json(json!(visible_git_summary(&policy, response))),
        ),
        Err(message) => error_response(message),
    }
}

async fn branches(auth: AuthUser, Query(query): Query<GitRootQuery>) -> (StatusCode, Json<Value>) {
    if let Some(project_id) = harness_project_id_from_root_path(query.root.as_str()) {
        return harness_git_branches(&auth, project_id).await;
    }
    let policy = match git_path_policy(&auth).await {
        Ok(policy) => policy,
        Err(err) => return err,
    };
    let root = match authorize_git_root(&policy, query.root.as_str(), false) {
        Ok(root) => root,
        Err(err) => return err,
    };
    match git::branches(root.as_str(), query.force_refresh.unwrap_or(false)).await {
        Ok(response) => (StatusCode::OK, Json(json!(response))),
        Err(message) => error_response(message),
    }
}

async fn status(auth: AuthUser, Query(query): Query<GitRootQuery>) -> (StatusCode, Json<Value>) {
    if let Some(project_id) = harness_project_id_from_root_path(query.root.as_str()) {
        if let Err(err) = ensure_harness_git_project(&auth, project_id).await {
            return err;
        }
        return (StatusCode::OK, Json(json!({ "files": [] })));
    }
    let policy = match git_path_policy(&auth).await {
        Ok(policy) => policy,
        Err(err) => return err,
    };
    let root = match authorize_git_root(&policy, query.root.as_str(), false) {
        Ok(root) => root,
        Err(err) => return err,
    };
    match git::status(root.as_str(), query.force_refresh.unwrap_or(false)).await {
        Ok(response) => (StatusCode::OK, Json(json!(response))),
        Err(message) => error_response(message),
    }
}

async fn compare(
    auth: AuthUser,
    Query(mut query): Query<GitCompareQuery>,
) -> (StatusCode, Json<Value>) {
    let policy = match git_path_policy(&auth).await {
        Ok(policy) => policy,
        Err(err) => return err,
    };
    query.root = match authorize_git_root(&policy, query.root.as_str(), false) {
        Ok(root) => root,
        Err(err) => return err,
    };
    match git::compare(query).await {
        Ok(response) => (StatusCode::OK, Json(json!(response))),
        Err(message) => error_response(message),
    }
}

async fn diff(auth: AuthUser, Query(mut query): Query<GitDiffQuery>) -> (StatusCode, Json<Value>) {
    let policy = match git_path_policy(&auth).await {
        Ok(policy) => policy,
        Err(err) => return err,
    };
    query.root = match authorize_git_root(&policy, query.root.as_str(), false) {
        Ok(root) => root,
        Err(err) => return err,
    };
    match git::file_diff(query).await {
        Ok(response) => (StatusCode::OK, Json(json!(response))),
        Err(message) => error_response(message),
    }
}

async fn fetch(
    auth: AuthUser,
    Json(mut request): Json<GitFetchRequest>,
) -> (StatusCode, Json<Value>) {
    let policy = match git_path_policy(&auth).await {
        Ok(policy) => policy,
        Err(err) => return err,
    };
    request.root = match authorize_git_root(&policy, request.root.as_str(), true) {
        Ok(root) => root,
        Err(err) => return err,
    };
    match git::fetch(request).await {
        Ok(response) => (
            StatusCode::OK,
            Json(json!(visible_git_action_result(&policy, response))),
        ),
        Err(message) => error_response(message),
    }
}

async fn pull(
    auth: AuthUser,
    Json(mut request): Json<GitPullRequest>,
) -> (StatusCode, Json<Value>) {
    let policy = match git_path_policy(&auth).await {
        Ok(policy) => policy,
        Err(err) => return err,
    };
    request.root = match authorize_git_root(&policy, request.root.as_str(), true) {
        Ok(root) => root,
        Err(err) => return err,
    };
    match git::pull(request).await {
        Ok(response) => (
            StatusCode::OK,
            Json(json!(visible_git_action_result(&policy, response))),
        ),
        Err(message) => error_response(message),
    }
}

async fn push(
    auth: AuthUser,
    Json(mut request): Json<GitPushRequest>,
) -> (StatusCode, Json<Value>) {
    let policy = match git_path_policy(&auth).await {
        Ok(policy) => policy,
        Err(err) => return err,
    };
    request.root = match authorize_git_root(&policy, request.root.as_str(), true) {
        Ok(root) => root,
        Err(err) => return err,
    };
    match git::push(request).await {
        Ok(response) => (
            StatusCode::OK,
            Json(json!(visible_git_action_result(&policy, response))),
        ),
        Err(message) => error_response(message),
    }
}

async fn checkout(
    auth: AuthUser,
    Json(mut request): Json<GitCheckoutRequest>,
) -> (StatusCode, Json<Value>) {
    let policy = match git_path_policy(&auth).await {
        Ok(policy) => policy,
        Err(err) => return err,
    };
    request.root = match authorize_git_root(&policy, request.root.as_str(), true) {
        Ok(root) => root,
        Err(err) => return err,
    };
    match git::checkout(request).await {
        Ok(response) => (
            StatusCode::OK,
            Json(json!(visible_git_action_result(&policy, response))),
        ),
        Err(message) => error_response(message),
    }
}

async fn create_branch(
    auth: AuthUser,
    Json(mut request): Json<GitCreateBranchRequest>,
) -> (StatusCode, Json<Value>) {
    let policy = match git_path_policy(&auth).await {
        Ok(policy) => policy,
        Err(err) => return err,
    };
    request.root = match authorize_git_root(&policy, request.root.as_str(), true) {
        Ok(root) => root,
        Err(err) => return err,
    };
    match git::create_branch(request).await {
        Ok(response) => (
            StatusCode::OK,
            Json(json!(visible_git_action_result(&policy, response))),
        ),
        Err(message) => error_response(message),
    }
}

async fn merge_branch(
    auth: AuthUser,
    Json(mut request): Json<GitMergeRequest>,
) -> (StatusCode, Json<Value>) {
    let policy = match git_path_policy(&auth).await {
        Ok(policy) => policy,
        Err(err) => return err,
    };
    request.root = match authorize_git_root(&policy, request.root.as_str(), true) {
        Ok(root) => root,
        Err(err) => return err,
    };
    match git::merge(request).await {
        Ok(response) => (
            StatusCode::OK,
            Json(json!(visible_git_action_result(&policy, response))),
        ),
        Err(message) => error_response(message),
    }
}

async fn stage(
    auth: AuthUser,
    Json(mut request): Json<GitPathRequest>,
) -> (StatusCode, Json<Value>) {
    let policy = match git_path_policy(&auth).await {
        Ok(policy) => policy,
        Err(err) => return err,
    };
    request.root = match authorize_git_root(&policy, request.root.as_str(), true) {
        Ok(root) => root,
        Err(err) => return err,
    };
    match git::stage(request).await {
        Ok(response) => (
            StatusCode::OK,
            Json(json!(visible_git_action_result(&policy, response))),
        ),
        Err(message) => error_response(message),
    }
}

async fn unstage(
    auth: AuthUser,
    Json(mut request): Json<GitPathRequest>,
) -> (StatusCode, Json<Value>) {
    let policy = match git_path_policy(&auth).await {
        Ok(policy) => policy,
        Err(err) => return err,
    };
    request.root = match authorize_git_root(&policy, request.root.as_str(), true) {
        Ok(root) => root,
        Err(err) => return err,
    };
    match git::unstage(request).await {
        Ok(response) => (
            StatusCode::OK,
            Json(json!(visible_git_action_result(&policy, response))),
        ),
        Err(message) => error_response(message),
    }
}

async fn commit(
    auth: AuthUser,
    Json(mut request): Json<GitCommitRequest>,
) -> (StatusCode, Json<Value>) {
    let policy = match git_path_policy(&auth).await {
        Ok(policy) => policy,
        Err(err) => return err,
    };
    request.root = match authorize_git_root(&policy, request.root.as_str(), true) {
        Ok(root) => root,
        Err(err) => return err,
    };
    match git::commit(request).await {
        Ok(response) => (
            StatusCode::OK,
            Json(json!(visible_git_action_result(&policy, response))),
        ),
        Err(message) => error_response(message),
    }
}

async fn discard(
    auth: AuthUser,
    Json(mut request): Json<GitPathRequest>,
) -> (StatusCode, Json<Value>) {
    let policy = match git_path_policy(&auth).await {
        Ok(policy) => policy,
        Err(err) => return err,
    };
    request.root = match authorize_git_root(&policy, request.root.as_str(), true) {
        Ok(root) => root,
        Err(err) => return err,
    };
    match git::discard(request).await {
        Ok(response) => (
            StatusCode::OK,
            Json(json!(visible_git_action_result(&policy, response))),
        ),
        Err(message) => error_response(message),
    }
}

async fn git_path_policy(auth: &AuthUser) -> Result<FsPathPolicy, (StatusCode, Json<Value>)> {
    FsPathPolicy::for_user(auth)
        .await
        .map_err(fs_policy_error_tuple)
}

fn fs_policy_error_tuple(err: FsPolicyError) -> (StatusCode, Json<Value>) {
    (
        err.status_code(),
        Json(serde_json::json!({ "error": err.message() })),
    )
}

fn authorize_git_root(
    policy: &FsPathPolicy,
    raw: &str,
    write: bool,
) -> Result<String, (StatusCode, Json<Value>)> {
    let trimmed = raw.trim();
    if parse_local_connector_root_path(trimmed).is_some() {
        return Ok(trimmed.to_string());
    }
    let authorized = policy
        .authorize_existing_dir(raw, "root 路径不存在", "root 不是目录")
        .map_err(fs_policy_error_tuple)?;
    if write {
        policy
            .require_write(&authorized)
            .map_err(fs_policy_error_tuple)?;
    }
    Ok(authorized.path.to_string_lossy().to_string())
}

fn authorize_optional_git_root(
    policy: &FsPathPolicy,
    raw: Option<&str>,
    write: bool,
) -> Result<Option<String>, (StatusCode, Json<Value>)> {
    let Some(raw) = raw.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(None);
    };
    authorize_git_root(policy, raw, write).map(Some)
}

fn visible_optional_path(policy: &FsPathPolicy, path: Option<String>) -> Option<String> {
    path.map(|path| policy.display_path(Path::new(path.as_str())))
}

fn visible_repository_candidate(
    policy: &FsPathPolicy,
    mut candidate: GitRepositoryCandidate,
) -> GitRepositoryCandidate {
    candidate.root = policy.display_path(Path::new(candidate.root.as_str()));
    candidate
}

fn visible_git_summary(policy: &FsPathPolicy, mut summary: GitSummary) -> GitSummary {
    summary.root = visible_optional_path(policy, summary.root);
    summary.worktree_root = visible_optional_path(policy, summary.worktree_root);
    summary.query_root = visible_optional_path(policy, summary.query_root);
    summary.resolved_root = visible_optional_path(policy, summary.resolved_root);
    summary.selected_root = visible_optional_path(policy, summary.selected_root);
    summary.available_repositories = summary
        .available_repositories
        .into_iter()
        .map(|candidate| visible_repository_candidate(policy, candidate))
        .collect();
    summary
}

fn visible_git_action_result(
    policy: &FsPathPolicy,
    mut result: GitActionResult,
) -> GitActionResult {
    result.summary = visible_git_summary(policy, result.summary);
    result
}

fn error_response(message: String) -> (StatusCode, Json<Value>) {
    let status = if message.contains("不能为空")
        || message.contains("不是")
        || message.contains("不存在")
        || message.contains("不合法")
        || message.contains("只能")
        || message.contains("不支持")
        || message.contains("不在")
        || message.contains("未完成")
        || message.contains("解析 root")
        || message.contains("不是 Git 仓库")
    {
        StatusCode::BAD_REQUEST
    } else {
        StatusCode::INTERNAL_SERVER_ERROR
    };
    (status, Json(json!({ "error": message })))
}

#[derive(Debug, Clone)]
struct HarnessBranchSnapshot {
    name: String,
    sha: String,
    is_default: bool,
}

#[derive(Debug, Clone)]
struct HarnessGitSnapshot {
    current: Option<String>,
    branches: Vec<HarnessBranchSnapshot>,
}

async fn ensure_harness_git_project(
    auth: &AuthUser,
    project_id: &str,
) -> Result<(), (StatusCode, Json<Value>)> {
    let project = ensure_owned_project(project_id, auth)
        .await
        .map_err(map_project_access_error)?;
    let is_cloud = project
        .source_type
        .as_deref()
        .map(str::trim)
        .is_some_and(|value| value.eq_ignore_ascii_case("cloud"));
    if !is_cloud {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "该项目不是云端项目" })),
        ));
    }
    Ok(())
}

async fn load_harness_git_snapshot(
    auth: &AuthUser,
    project_id: &str,
) -> Result<HarnessGitSnapshot, (StatusCode, Json<Value>)> {
    ensure_harness_git_project(auth, project_id).await?;
    let cfg = Config::try_get().map_err(|err| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": err })),
        )
    })?;
    let sync_secret = cfg
        .project_service_sync_secret
        .as_deref()
        .or(cfg.task_runner_callback_secret.as_deref())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "project service sync secret is not configured" })),
            )
        })?;
    let value = project_management_api_client::call_project_harness_tool(
        cfg.project_service_base_url.as_str(),
        sync_secret,
        project_id,
        "list_branches",
        json!({}),
    )
    .await
    .map_err(|err| (StatusCode::BAD_GATEWAY, Json(json!({ "error": err }))))?;
    let branches = value
        .get("branches")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| {
                    let name = item.get("name").and_then(Value::as_str)?.trim();
                    if name.is_empty() {
                        return None;
                    }
                    Some(HarnessBranchSnapshot {
                        name: name.to_string(),
                        sha: item
                            .get("sha")
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .to_string(),
                        is_default: item
                            .get("is_default")
                            .and_then(Value::as_bool)
                            .unwrap_or(false),
                    })
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let current = value
        .get("current")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .or_else(|| {
            branches
                .iter()
                .find(|branch| branch.is_default)
                .or_else(|| branches.first())
                .map(|branch| branch.name.clone())
        });
    Ok(HarnessGitSnapshot { current, branches })
}

async fn harness_git_summary(
    auth: &AuthUser,
    project_id: &str,
    root: &str,
) -> (StatusCode, Json<Value>) {
    let snapshot = match load_harness_git_snapshot(auth, project_id).await {
        Ok(snapshot) => snapshot,
        Err(err) => return err,
    };
    let head = snapshot.current.as_deref().and_then(|current| {
        snapshot
            .branches
            .iter()
            .find(|branch| branch.name == current)
            .map(|branch| branch.sha.clone())
    });
    (
        StatusCode::OK,
        Json(json!({
            "is_repo": true,
            "root": root,
            "worktree_root": Value::Null,
            "query_root": root,
            "resolved_root": root,
            "selected_root": root,
            "head": head,
            "current_branch": snapshot.current,
            "detached": false,
            "upstream": "Harness",
            "ahead": 0,
            "behind": 0,
            "dirty": false,
            "operation_state": Value::Null,
            "changes": {
                "staged": 0,
                "unstaged": 0,
                "untracked": 0,
                "conflicted": 0,
            },
            "available_repositories": [],
        })),
    )
}

async fn harness_git_branches(auth: &AuthUser, project_id: &str) -> (StatusCode, Json<Value>) {
    let snapshot = match load_harness_git_snapshot(auth, project_id).await {
        Ok(snapshot) => snapshot,
        Err(err) => return err,
    };
    let current = snapshot.current.clone();
    let locals = snapshot
        .branches
        .into_iter()
        .map(|branch| {
            let is_current = current.as_deref() == Some(branch.name.as_str());
            json!({
                "name": branch.name,
                "short_name": Value::Null,
                "current": is_current,
                "upstream": Value::Null,
                "remote": Value::Null,
                "tracked_by": Value::Null,
                "ahead": 0,
                "behind": 0,
                "last_commit": branch.sha,
                "last_commit_subject": Value::Null,
            })
        })
        .collect::<Vec<_>>();
    (
        StatusCode::OK,
        Json(json!({
            "current": current,
            "locals": locals,
            "remotes": [],
        })),
    )
}
