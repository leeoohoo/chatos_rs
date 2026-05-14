use axum::http::StatusCode;
use axum::{extract::Query, Json};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::core::auth::AuthUser;
use crate::core::user_scope::resolve_user_id;
use crate::modules::conversation_runtime::tools_panel::{
    build_v3_agent_tools_panel, load_agent_status_runtime_panel,
};

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
    match build_v3_agent_tools_panel(user_id.as_str()).await {
        Ok(panel) => (StatusCode::OK, Json(json!(panel))),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": err })),
        ),
    }
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
    let runtime_panel = load_agent_status_runtime_panel(user_id).await;
    Json(json!({
        "status": "ok",
        "version": "3.0.0",
        "timestamp": crate::core::time::now_rfc3339(),
        "openai": {
            "configured": !cfg.openai_api_key.is_empty(),
            "base_url": cfg.openai_base_url.clone()
        },
        "servers": runtime_panel.servers,
        "builtin_mcp_prompt_debug": runtime_panel.builtin_mcp_prompt_debug,
    }))
}
