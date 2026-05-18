use axum::http::StatusCode;
use axum::{
    extract::{Path, Query},
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::api::chat_stream_common::{validate_chat_stream_request, ChatStreamRequest};
use crate::api::conversation_semantics::extract_conversation_scope_id;
use crate::config::Config;
use crate::core::auth::AuthUser;
use crate::core::session_access::{ensure_owned_session, map_session_access_error};
use crate::core::user_scope::{ensure_and_set_user_id, resolve_user_id};
use crate::modules::conversation_runtime::chat_usecase::{
    run_chat_v2_usecase, RunChatV2UsecaseInput,
};
use crate::modules::conversation_runtime::messages as conversation_messages;
use crate::modules::conversation_runtime::tools_panel::{
    build_v2_agent_tools_panel, load_agent_status_runtime_panel,
};
use crate::services::access_token_scope;
use crate::services::ai_common::normalize_turn_id;
use crate::utils::abort_registry;
use crate::utils::sse::SseSender;

#[derive(Debug, Deserialize)]
struct UserQuery {
    user_id: Option<String>,
}

pub fn router() -> Router {
    Router::new()
        .route("/api/agent_v2/chat/send", post(agent_chat_send))
        .route("/api/agent_v2/tools", get(agent_tools))
        .route("/api/agent_v2/status", get(agent_status))
        .route(
            "/api/agent_v2/conversation/:conversation_id/reset",
            post(reset_conversation),
        )
        .route("/api/chat/stop", post(stop_chat))
}

async fn agent_chat_send(
    auth: AuthUser,
    Json(mut req): Json<ChatStreamRequest>,
) -> Result<(StatusCode, Json<Value>), (StatusCode, Json<Value>)> {
    if let Err(err) = ensure_and_set_user_id(&mut req.user_id, &auth) {
        return Err(err);
    }
    validate_chat_stream_request(&req, false).await?;
    let conversation_id = req.conversation_id.clone().unwrap_or_default();
    let accepted_turn_id = normalize_turn_id(req.turn_id.as_deref());

    abort_registry::reset(&conversation_id);
    access_token_scope::spawn_with_current_access_token(stream_chat_v2(
        None, req, false, true, false,
    ));

    Ok((
        StatusCode::ACCEPTED,
        Json(json!({
            "accepted": true,
            "conversation_id": conversation_id,
            "turn_id": accepted_turn_id,
        })),
    ))
}

async fn agent_tools(auth: AuthUser, Query(query): Query<UserQuery>) -> (StatusCode, Json<Value>) {
    let user_id = match resolve_user_id(query.user_id, &auth) {
        Ok(user_id) => user_id,
        Err(err) => return err,
    };
    match build_v2_agent_tools_panel(user_id.as_str()).await {
        Ok(panel) => (StatusCode::OK, Json(json!(panel))),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": err })),
        ),
    }
}

async fn agent_status(auth: AuthUser, Query(query): Query<UserQuery>) -> Json<Value> {
    let cfg = match Config::try_get() {
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
        "version": "2.0.0",
        "timestamp": crate::core::time::now_rfc3339(),
        "openai": {
            "configured": !cfg.openai_api_key.is_empty(),
            "base_url": cfg.openai_base_url.clone()
        },
        "servers": runtime_panel.servers,
        "builtin_mcp_prompt_debug": runtime_panel.builtin_mcp_prompt_debug,
    }))
}

async fn reset_conversation(
    auth: AuthUser,
    Path(conversation_id): Path<String>,
) -> (StatusCode, Json<Value>) {
    if let Err(err) = ensure_owned_session(&conversation_id, &auth).await {
        return map_session_access_error(err);
    }
    match conversation_messages::delete_messages_by_session(&conversation_id).await {
        Ok(_) => (
            StatusCode::OK,
            Json(json!({
                "success": true,
                "message": "对话线程重置成功",
                "conversation_id": conversation_id
            })),
        ),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({
                "success": false,
                "message": "重置对话线程失败",
                "detail": err,
                "conversation_id": conversation_id
            })),
        ),
    }
}

async fn stop_chat(Json(req): Json<Value>) -> (StatusCode, Json<Value>) {
    let conversation_id = extract_conversation_scope_id(&req).unwrap_or_default();
    if conversation_id.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"success": false, "message": "缺少 conversation_id"})),
        );
    }
    let ok = abort_registry::abort(conversation_id.as_str());
    if ok {
        return (
            StatusCode::OK,
            Json(json!({
                "success": true,
                "message": "停止中",
                "conversation_id": conversation_id
            })),
        );
    }
    (
        StatusCode::OK,
        Json(json!({
            "success": false,
            "message": "未找到可停止的对话线程或已停止",
            "conversation_id": conversation_id
        })),
    )
}

async fn stream_chat_v2(
    sender: Option<SseSender>,
    req: ChatStreamRequest,
    always_send_done: bool,
    rename_session: bool,
    respect_model_flags: bool,
) {
    run_chat_v2_usecase(RunChatV2UsecaseInput {
        sender,
        req,
        always_send_done,
        rename_session,
        respect_model_flags,
    })
    .await;
}
