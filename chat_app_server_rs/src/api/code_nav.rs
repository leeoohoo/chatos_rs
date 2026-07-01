use axum::http::StatusCode;
use axum::routing::post;
use axum::{Json, Router};
use once_cell::sync::Lazy;
use serde_json::{json, Value};

use crate::api::fs::policy::{FsPathPolicy, FsPolicyError};
use crate::core::auth::AuthUser;
use crate::core::path_guard::path_is_within_root;
use crate::services::code_nav::manager::CodeNavManager;
use crate::services::code_nav::types::{DocumentSymbolsRequest, NavPositionRequest};

static CODE_NAV_MANAGER: Lazy<CodeNavManager> = Lazy::new(CodeNavManager::default);

pub fn router() -> Router {
    Router::new()
        .route("/api/code-nav/capabilities", post(capabilities))
        .route("/api/code-nav/definition", post(definition))
        .route("/api/code-nav/references", post(references))
        .route("/api/code-nav/document-symbols", post(document_symbols))
}

async fn capabilities(
    auth: AuthUser,
    Json(request): Json<DocumentSymbolsRequest>,
) -> (StatusCode, Json<Value>) {
    let request = match authorize_document_symbols_request(&auth, request).await {
        Ok(request) => request,
        Err(err) => return err,
    };
    match CODE_NAV_MANAGER
        .capabilities(&request.project_root, &request.file_path)
        .await
    {
        Ok(response) => (StatusCode::OK, Json(json!(response))),
        Err(message) => error_response(message),
    }
}

async fn definition(
    auth: AuthUser,
    Json(request): Json<NavPositionRequest>,
) -> (StatusCode, Json<Value>) {
    let request = match authorize_nav_position_request(&auth, request).await {
        Ok(request) => request,
        Err(err) => return err,
    };
    match CODE_NAV_MANAGER.definition(&request).await {
        Ok(response) => (StatusCode::OK, Json(json!(response))),
        Err(message) => error_response(message),
    }
}

async fn references(
    auth: AuthUser,
    Json(request): Json<NavPositionRequest>,
) -> (StatusCode, Json<Value>) {
    let request = match authorize_nav_position_request(&auth, request).await {
        Ok(request) => request,
        Err(err) => return err,
    };
    match CODE_NAV_MANAGER.references(&request).await {
        Ok(response) => (StatusCode::OK, Json(json!(response))),
        Err(message) => error_response(message),
    }
}

async fn document_symbols(
    auth: AuthUser,
    Json(request): Json<DocumentSymbolsRequest>,
) -> (StatusCode, Json<Value>) {
    let request = match authorize_document_symbols_request(&auth, request).await {
        Ok(request) => request,
        Err(err) => return err,
    };
    match CODE_NAV_MANAGER.document_symbols(&request).await {
        Ok(response) => (StatusCode::OK, Json(json!(response))),
        Err(message) => error_response(message),
    }
}

fn fs_policy_error_tuple(err: FsPolicyError) -> (StatusCode, Json<Value>) {
    (
        err.status_code(),
        Json(serde_json::json!({ "error": err.message() })),
    )
}

async fn authorize_code_nav_paths(
    auth: &AuthUser,
    project_root: &str,
    file_path: &str,
) -> Result<(String, String), (StatusCode, Json<Value>)> {
    let policy = FsPathPolicy::for_user(auth)
        .await
        .map_err(fs_policy_error_tuple)?;
    let root = policy
        .authorize_existing_dir(project_root, "project_root 不存在", "project_root 不是目录")
        .map_err(fs_policy_error_tuple)?;
    let file = policy
        .authorize_existing_file(file_path, "file_path 不存在", "file_path 不是文件")
        .map_err(fs_policy_error_tuple)?;
    if !path_is_within_root(file.path.as_path(), root.path.as_path()) {
        return Err((
            StatusCode::FORBIDDEN,
            Json(json!({ "error": "file_path 超出项目根目录" })),
        ));
    }
    Ok((
        root.path.to_string_lossy().to_string(),
        file.path.to_string_lossy().to_string(),
    ))
}

async fn authorize_document_symbols_request(
    auth: &AuthUser,
    request: DocumentSymbolsRequest,
) -> Result<DocumentSymbolsRequest, (StatusCode, Json<Value>)> {
    let (project_root, file_path) =
        authorize_code_nav_paths(auth, &request.project_root, &request.file_path).await?;
    Ok(DocumentSymbolsRequest {
        project_root,
        file_path,
    })
}

async fn authorize_nav_position_request(
    auth: &AuthUser,
    request: NavPositionRequest,
) -> Result<NavPositionRequest, (StatusCode, Json<Value>)> {
    let (project_root, file_path) =
        authorize_code_nav_paths(auth, &request.project_root, &request.file_path).await?;
    Ok(NavPositionRequest {
        project_root,
        file_path,
        line: request.line,
        column: request.column,
    })
}

fn error_response(message: String) -> (StatusCode, Json<Value>) {
    let status = if message.contains("不存在")
        || message.contains("不是目录")
        || message.contains("不是文件")
        || message.contains("不能为空")
        || message.contains("超出项目根目录")
    {
        StatusCode::BAD_REQUEST
    } else {
        StatusCode::INTERNAL_SERVER_ERROR
    };
    (status, Json(json!({ "error": message })))
}
