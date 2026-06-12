use axum::{Json, extract::Path, http::StatusCode};
use serde_json::Value;

use crate::api::conversation_semantics::rewrite_session_keys_to_conversation;
use crate::core::auth::AuthUser;
use crate::core::session_access::{ensure_owned_session, map_session_access_error};
use crate::modules::conversation_runtime::session_mcp_servers as conversation_session_mcp_servers;

use super::contracts::AddMcpServerRequest;

pub(super) async fn list_mcp_servers(
    auth: AuthUser,
    Path(conversation_id): Path<String>,
) -> (StatusCode, Json<Value>) {
    if let Err(err) = ensure_owned_session(&conversation_id, &auth).await {
        return map_session_access_error(err);
    }
    match conversation_session_mcp_servers::list_session_mcp_servers(&conversation_id).await {
        Ok(res) => (
            StatusCode::OK,
            Json(rewrite_session_keys_to_conversation(
                serde_json::to_value(res).unwrap_or(Value::Null),
            )),
        ),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "获取对话线程MCP服务器失败", "detail": err})),
        ),
    }
}

pub(super) async fn add_mcp_server(
    auth: AuthUser,
    Path(conversation_id): Path<String>,
    Json(req): Json<AddMcpServerRequest>,
) -> (StatusCode, Json<Value>) {
    if let Err(err) = ensure_owned_session(&conversation_id, &auth).await {
        return map_session_access_error(err);
    }
    match conversation_session_mcp_servers::add_session_mcp_server(
        conversation_session_mcp_servers::AddSessionMcpServerInput {
            session_id: conversation_id.clone(),
            mcp_server_name: req.mcp_server_name.clone(),
            mcp_config_id: req.mcp_config_id.clone(),
        },
    )
    .await
    {
        Ok(item) => (
            StatusCode::CREATED,
            Json(serde_json::json!({
                "id": item.id,
                "conversation_id": conversation_id,
                "conversationId": conversation_id,
                "mcp_server_name": item.mcp_server_name,
                "mcp_config_id": item.mcp_config_id
            })),
        ),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "添加对话线程MCP服务器失败", "detail": err})),
        ),
    }
}

pub(super) async fn delete_mcp_server(
    auth: AuthUser,
    Path((conversation_id, mcp_config_id)): Path<(String, String)>,
) -> (StatusCode, Json<Value>) {
    if let Err(err) = ensure_owned_session(&conversation_id, &auth).await {
        return map_session_access_error(err);
    }
    match conversation_session_mcp_servers::delete_session_mcp_server(
        &conversation_id,
        &mcp_config_id,
    )
    .await
    {
        Ok(_) => (
            StatusCode::OK,
            Json(
                serde_json::json!({"success": true, "conversation_id": conversation_id, "conversationId": conversation_id}),
            ),
        ),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "删除对话线程MCP服务器关联失败", "detail": err})),
        ),
    }
}
