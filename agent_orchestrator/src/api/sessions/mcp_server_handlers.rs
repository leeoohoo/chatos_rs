use axum::{extract::Path, http::StatusCode, Json};
use serde_json::Value;
use uuid::Uuid;

use crate::core::auth::AuthUser;
use crate::core::session_access::{ensure_owned_session, map_session_access_error};
use crate::models::session_mcp_server::SessionMcpServer;
use crate::repositories::session_mcp_servers as session_mcp_repo;

use super::contracts::AddMcpServerRequest;

pub(super) async fn list_mcp_servers(
    auth: AuthUser,
    Path(session_id): Path<String>,
) -> (StatusCode, Json<Value>) {
    if let Err(err) = ensure_owned_session(&session_id, &auth).await {
        return map_session_access_error(err);
    }
    match session_mcp_repo::list_session_mcp_servers(&session_id).await {
        Ok(res) => (
            StatusCode::OK,
            Json(serde_json::to_value(res).unwrap_or(Value::Null)),
        ),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "获取会话MCP服务器失败", "detail": err})),
        ),
    }
}

pub(super) async fn add_mcp_server(
    auth: AuthUser,
    Path(session_id): Path<String>,
    Json(req): Json<AddMcpServerRequest>,
) -> (StatusCode, Json<Value>) {
    if let Err(err) = ensure_owned_session(&session_id, &auth).await {
        return map_session_access_error(err);
    }
    let id = Uuid::new_v4().to_string();
    let item = SessionMcpServer {
        id: id.clone(),
        session_id: session_id.clone(),
        mcp_server_name: req.mcp_server_name.clone(),
        mcp_config_id: req.mcp_config_id.clone(),
        created_at: crate::core::time::now_rfc3339(),
    };
    if let Err(err) = session_mcp_repo::add_session_mcp_server(&item).await {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "添加会话MCP服务器失败", "detail": err})),
        );
    }
    (
        StatusCode::CREATED,
        Json(
            serde_json::json!({"id": id, "session_id": session_id, "mcp_server_name": req.mcp_server_name, "mcp_config_id": req.mcp_config_id}),
        ),
    )
}

pub(super) async fn delete_mcp_server(
    auth: AuthUser,
    Path((session_id, mcp_config_id)): Path<(String, String)>,
) -> (StatusCode, Json<Value>) {
    if let Err(err) = ensure_owned_session(&session_id, &auth).await {
        return map_session_access_error(err);
    }
    match session_mcp_repo::delete_session_mcp_server(&session_id, &mcp_config_id).await {
        Ok(_) => (StatusCode::OK, Json(serde_json::json!({"success": true}))),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "删除会话MCP服务器关联失败", "detail": err})),
        ),
    }
}
