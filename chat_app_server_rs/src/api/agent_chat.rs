#[path = "agent_chat/runtime_guidance.rs"]
mod runtime_guidance;
#[path = "agent_chat/tools_panel.rs"]
mod tools_panel;

use axum::http::StatusCode;
use axum::{
    extract::Path,
    routing::{get, post},
    Json, Router,
};
use serde_json::{json, Value};

use self::runtime_guidance::submit_runtime_guidance;
use self::tools_panel::{agent_status, agent_tools};
use crate::api::chat_stream_common::{validate_chat_stream_request, ChatStreamRequest};
use crate::api::conversation_semantics::extract_conversation_scope_id;
use crate::core::auth::AuthUser;
use crate::core::session_access::{ensure_owned_session, map_session_access_error};
use crate::core::user_scope::ensure_and_set_user_id;
use crate::modules::conversation_runtime::chat_usecase::{run_chat_usecase, RunChatUsecaseInput};
use crate::modules::conversation_runtime::messages as conversation_messages;
use crate::services::access_token_scope;
use crate::services::ai_common::normalize_turn_id;
use crate::utils::abort_registry;
use crate::utils::sse::SseSender;

pub fn router() -> Router {
    Router::new()
        .route("/api/agent/chat/send", post(agent_chat_send))
        .route("/api/agent/chat/stop", post(stop_chat))
        .route("/api/agent/chat/guide", post(submit_runtime_guidance))
        .route("/api/agent/tools", get(agent_tools))
        .route("/api/agent/status", get(agent_status))
        .route(
            "/api/agent/conversation/:conversation_id/reset",
            post(reset_conversation),
        )
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
    access_token_scope::spawn_with_current_access_token(stream_chat(None, req));

    Ok((
        StatusCode::ACCEPTED,
        Json(json!({
            "accepted": true,
            "conversation_id": conversation_id,
            "turn_id": accepted_turn_id,
        })),
    ))
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

async fn stream_chat(sender: Option<SseSender>, req: ChatStreamRequest) {
    run_chat_usecase(RunChatUsecaseInput { sender, req }).await;
}
