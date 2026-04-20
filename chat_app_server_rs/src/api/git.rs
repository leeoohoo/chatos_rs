use axum::extract::Query;
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde_json::{json, Value};

use crate::services::git;
use crate::services::git::contracts::{
    GitCheckoutRequest, GitCommitRequest, GitCompareQuery, GitCreateBranchRequest, GitDiffQuery,
    GitFetchRequest, GitMergeRequest, GitPathRequest, GitPullRequest, GitPushRequest, GitRootQuery,
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
        .route("/api/git/commit", post(commit))
}

async fn client() -> (StatusCode, Json<Value>) {
    (StatusCode::OK, Json(json!(git::client_info().await)))
}

async fn summary(Query(query): Query<GitRootQuery>) -> (StatusCode, Json<Value>) {
    match git::summary(&query.root).await {
        Ok(response) => (StatusCode::OK, Json(json!(response))),
        Err(message) => error_response(message),
    }
}

async fn branches(Query(query): Query<GitRootQuery>) -> (StatusCode, Json<Value>) {
    match git::branches(&query.root).await {
        Ok(response) => (StatusCode::OK, Json(json!(response))),
        Err(message) => error_response(message),
    }
}

async fn status(Query(query): Query<GitRootQuery>) -> (StatusCode, Json<Value>) {
    match git::status(&query.root).await {
        Ok(response) => (StatusCode::OK, Json(json!(response))),
        Err(message) => error_response(message),
    }
}

async fn compare(Query(query): Query<GitCompareQuery>) -> (StatusCode, Json<Value>) {
    match git::compare(query).await {
        Ok(response) => (StatusCode::OK, Json(json!(response))),
        Err(message) => error_response(message),
    }
}

async fn diff(Query(query): Query<GitDiffQuery>) -> (StatusCode, Json<Value>) {
    match git::file_diff(query).await {
        Ok(response) => (StatusCode::OK, Json(json!(response))),
        Err(message) => error_response(message),
    }
}

async fn fetch(Json(request): Json<GitFetchRequest>) -> (StatusCode, Json<Value>) {
    match git::fetch(request).await {
        Ok(response) => (StatusCode::OK, Json(json!(response))),
        Err(message) => error_response(message),
    }
}

async fn pull(Json(request): Json<GitPullRequest>) -> (StatusCode, Json<Value>) {
    match git::pull(request).await {
        Ok(response) => (StatusCode::OK, Json(json!(response))),
        Err(message) => error_response(message),
    }
}

async fn push(Json(request): Json<GitPushRequest>) -> (StatusCode, Json<Value>) {
    match git::push(request).await {
        Ok(response) => (StatusCode::OK, Json(json!(response))),
        Err(message) => error_response(message),
    }
}

async fn checkout(Json(request): Json<GitCheckoutRequest>) -> (StatusCode, Json<Value>) {
    match git::checkout(request).await {
        Ok(response) => (StatusCode::OK, Json(json!(response))),
        Err(message) => error_response(message),
    }
}

async fn create_branch(Json(request): Json<GitCreateBranchRequest>) -> (StatusCode, Json<Value>) {
    match git::create_branch(request).await {
        Ok(response) => (StatusCode::OK, Json(json!(response))),
        Err(message) => error_response(message),
    }
}

async fn merge_branch(Json(request): Json<GitMergeRequest>) -> (StatusCode, Json<Value>) {
    match git::merge(request).await {
        Ok(response) => (StatusCode::OK, Json(json!(response))),
        Err(message) => error_response(message),
    }
}

async fn stage(Json(request): Json<GitPathRequest>) -> (StatusCode, Json<Value>) {
    match git::stage(request).await {
        Ok(response) => (StatusCode::OK, Json(json!(response))),
        Err(message) => error_response(message),
    }
}

async fn unstage(Json(request): Json<GitPathRequest>) -> (StatusCode, Json<Value>) {
    match git::unstage(request).await {
        Ok(response) => (StatusCode::OK, Json(json!(response))),
        Err(message) => error_response(message),
    }
}

async fn commit(Json(request): Json<GitCommitRequest>) -> (StatusCode, Json<Value>) {
    match git::commit(request).await {
        Ok(response) => (StatusCode::OK, Json(json!(response))),
        Err(message) => error_response(message),
    }
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
