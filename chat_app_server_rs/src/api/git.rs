// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::extract::Query;
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde_json::{json, Value};
use std::path::Path;

use crate::api::fs::policy::{FsPathPolicy, FsPolicyError};
use crate::core::auth::AuthUser;
use crate::services::git;
use crate::services::git::{
    GitActionResult, GitCheckoutRequest, GitCommitRequest, GitCompareQuery, GitCreateBranchRequest,
    GitDiffQuery, GitFetchRequest, GitMergeRequest, GitPathRequest, GitPullRequest, GitPushRequest,
    GitRepositoryCandidate, GitRootQuery, GitSummary,
};

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
