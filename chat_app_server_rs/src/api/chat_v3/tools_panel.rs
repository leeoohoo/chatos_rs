use std::collections::HashMap;

use axum::http::StatusCode;
use axum::{
    extract::Query,
    Json,
};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::core::auth::AuthUser;
use crate::core::builtin_mcp_prompt::compose_effective_builtin_mcp_system_prompt;
use crate::core::mcp_runtime::{load_mcp_servers_by_selection, McpServerBundle};
use crate::core::mcp_tools::ToolInfo;
use crate::core::user_scope::resolve_user_id;
use crate::services::v3::mcp_tool_execute::McpToolExecute;
use crate::api::chat_stream_common::build_builtin_mcp_debug_payload;

#[derive(Debug, Deserialize)]
pub(super) struct UserQuery {
    pub(super) user_id: Option<String>,
}

pub(super) async fn agent_tools(
    auth: AuthUser,
    Query(query): Query<UserQuery>,
) -> (StatusCode, Json<Value>) {
    let user_id = match resolve_user_id(query.user_id, &auth) {
        Ok(user_id) => user_id,
        Err(err) => return err,
    };
    let (http_servers, stdio_servers, builtin_servers): McpServerBundle =
        load_mcp_servers_by_selection(Some(user_id), false, Vec::new(), None, None).await;
    let mut exec = McpToolExecute::new(
        http_servers.clone(),
        stdio_servers.clone(),
        builtin_servers.clone(),
    );
    if let Err(err) = exec.init().await {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": err })),
        );
    }
    let tools = exec.get_tools();
    let unavailable_tools = exec.get_unavailable_tools();
    let builtin_prompt_debug = build_builtin_mcp_debug_payload(
        builtin_servers.as_slice(),
        exec.tool_metadata(),
        unavailable_tools.as_slice(),
        Some(
            compose_effective_builtin_mcp_system_prompt(
                builtin_servers.as_slice(),
                exec.tool_metadata(),
                unavailable_tools.as_slice(),
            )
            .unwrap_or_default()
            .as_str(),
        ),
    );
    (
        StatusCode::OK,
        Json(json!({
            "tools": tools,
            "count": tools.len(),
            "unavailable_tools": unavailable_tools,
            "unavailable_count": unavailable_tools.len(),
            "servers": { "http": http_servers.len(), "stdio": stdio_servers.len(), "builtin": builtin_servers.len() },
            "builtin_mcp_prompt_debug": builtin_prompt_debug,
        })),
    )
}

pub(super) async fn agent_status(auth: AuthUser, Query(query): Query<UserQuery>) -> Json<Value> {
    let cfg = match crate::config::Config::try_get() {
        Ok(cfg) => cfg,
        Err(err) => {
            return Json(json!({
                "status": "error",
                "error": "服务配置未初始化",
                "detail": err
            }));
        }
    };
    let user_id = resolve_user_id(query.user_id, &auth).ok();
    let (http_servers, stdio_servers, builtin_servers): McpServerBundle =
        load_mcp_servers_by_selection(user_id, false, Vec::new(), None, None).await;
    let builtin_prompt_debug = build_builtin_mcp_debug_payload(
        builtin_servers.as_slice(),
        &HashMap::<String, ToolInfo>::new(),
        &[],
        None,
    );
    Json(json!({
        "status": "ok",
        "version": "3.0.0",
        "timestamp": crate::core::time::now_rfc3339(),
        "openai": {
            "configured": !cfg.openai_api_key.is_empty(),
            "base_url": cfg.openai_base_url.clone()
        },
        "servers": { "http": http_servers.len(), "stdio": stdio_servers.len(), "builtin": builtin_servers.len() },
        "builtin_mcp_prompt_debug": builtin_prompt_debug,
    }))
}
