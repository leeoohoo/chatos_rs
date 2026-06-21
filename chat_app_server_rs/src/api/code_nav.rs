use axum::http::StatusCode;
use axum::routing::post;
use axum::{Json, Router};
use once_cell::sync::Lazy;
use serde_json::{json, Value};

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

async fn capabilities(Json(request): Json<DocumentSymbolsRequest>) -> (StatusCode, Json<Value>) {
    match CODE_NAV_MANAGER
        .capabilities(&request.project_root, &request.file_path)
        .await
    {
        Ok(response) => (StatusCode::OK, Json(json!(response))),
        Err(message) => error_response(message),
    }
}

async fn definition(Json(request): Json<NavPositionRequest>) -> (StatusCode, Json<Value>) {
    match CODE_NAV_MANAGER.definition(&request).await {
        Ok(response) => (StatusCode::OK, Json(json!(response))),
        Err(message) => error_response(message),
    }
}

async fn references(Json(request): Json<NavPositionRequest>) -> (StatusCode, Json<Value>) {
    match CODE_NAV_MANAGER.references(&request).await {
        Ok(response) => (StatusCode::OK, Json(json!(response))),
        Err(message) => error_response(message),
    }
}

async fn document_symbols(
    Json(request): Json<DocumentSymbolsRequest>,
) -> (StatusCode, Json<Value>) {
    match CODE_NAV_MANAGER.document_symbols(&request).await {
        Ok(response) => (StatusCode::OK, Json(json!(response))),
        Err(message) => error_response(message),
    }
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
