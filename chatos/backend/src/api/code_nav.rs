// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::http::StatusCode;
use axum::routing::post;
use axum::{Json, Router};
use once_cell::sync::Lazy;
use serde_json::{json, Value};

use crate::api::fs::policy::{FsPathPolicy, FsPolicyError};
use crate::api::local_connectors::parse_local_connector_root_path;
use crate::core::auth::AuthUser;
use crate::core::path_guard::path_is_within_root;
use crate::services::code_nav::manager::CodeNavManager;
use crate::services::code_nav::types::{
    DocumentSymbolsRequest, NavLocationsResponse, NavPositionRequest,
};

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
        Ok(response) => (
            StatusCode::OK,
            Json(json!(visible_nav_locations_response(response))),
        ),
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
        Ok(response) => (
            StatusCode::OK,
            Json(json!(visible_nav_locations_response(response))),
        ),
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

fn visible_nav_locations_response(mut response: NavLocationsResponse) -> NavLocationsResponse {
    for location in &mut response.locations {
        location.path = crate::core::user_visible_path::display_path(location.path.as_str());
    }
    response
}

async fn authorize_code_nav_paths(
    auth: &AuthUser,
    project_root: &str,
    file_path: &str,
) -> Result<(String, String), (StatusCode, Json<Value>)> {
    if parse_local_connector_root_path(project_root).is_some()
        || parse_local_connector_root_path(file_path).is_some()
    {
        return authorize_local_connector_code_nav_paths(project_root, file_path);
    }

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

fn authorize_local_connector_code_nav_paths(
    project_root: &str,
    file_path: &str,
) -> Result<(String, String), (StatusCode, Json<Value>)> {
    let project_root = project_root.trim();
    let file_path = file_path.trim();
    let Some(root_ref) = parse_local_connector_root_path(project_root) else {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "project_root 格式错误" })),
        ));
    };
    let Some(file_ref) = parse_local_connector_root_path(file_path) else {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "file_path 格式错误" })),
        ));
    };
    if root_ref.device_id != file_ref.device_id || root_ref.workspace_id != file_ref.workspace_id {
        return Err((
            StatusCode::FORBIDDEN,
            Json(json!({ "error": "file_path 超出项目根目录" })),
        ));
    }

    let root_relative = root_ref.relative_path.unwrap_or_default();
    let file_relative = file_ref.relative_path.unwrap_or_default();
    let in_project = root_relative.is_empty()
        || file_relative == root_relative
        || file_relative
            .strip_prefix(root_relative.as_str())
            .is_some_and(|rest| rest.starts_with('/'));
    if !in_project {
        return Err((
            StatusCode::FORBIDDEN,
            Json(json!({ "error": "file_path 超出项目根目录" })),
        ));
    }
    if file_relative.is_empty() || file_relative == root_relative {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "file_path 不是文件" })),
        ));
    }

    Ok((project_root.to_string(), file_path.to_string()))
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
