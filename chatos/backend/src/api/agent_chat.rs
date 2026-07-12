// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

mod task_runner_callback;
#[path = "agent_chat/tools_panel.rs"]
mod tools_panel;

use axum::http::StatusCode;
use axum::{
    extract::Path,
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

use self::task_runner_callback::task_runner_callback;
use self::tools_panel::{agent_status, agent_tools};
use crate::api::chat_stream_common::{validate_chat_stream_request, ChatStreamRequest};
use crate::api::conversation_semantics::extract_conversation_scope_id;
use crate::core::auth::AuthUser;
use crate::core::messages::{build_message, MessageOut, NewMessageFields};
use crate::core::session_access::{ensure_owned_session, map_session_access_error};
use crate::core::user_scope::ensure_and_set_user_id;
use crate::modules::conversation_runtime::chat_usecase::{run_chat_usecase, RunChatUsecaseInput};
use crate::modules::conversation_runtime::guidance;
use crate::modules::conversation_runtime::messages as conversation_messages;
use crate::services::access_token_scope;
use crate::services::ai_common::normalize_turn_id;
use crate::services::chatos_sessions;
use crate::utils::abort_registry;
use crate::utils::attachments;
use crate::utils::sse::SseSender;

#[derive(Debug, Deserialize)]
struct RuntimeGuidanceRequest {
    #[serde(rename = "conversation_id", alias = "conversationId")]
    conversation_id: Option<String>,
    #[serde(rename = "turn_id", alias = "turnId")]
    turn_id: Option<String>,
    content: Option<String>,
    attachments: Option<Vec<Value>>,
}

pub fn router() -> Router {
    Router::new()
        .route("/api/agent/chat/send", post(agent_chat_send))
        .route("/api/agent/chat/guidance", post(agent_chat_guidance))
        .route("/api/agent/chat/stop", post(stop_chat))
        .route("/api/agent/tools", get(agent_tools))
        .route("/api/agent/status", get(agent_status))
        .route(
            "/api/agent/conversation/{conversation_id}/reset",
            post(reset_conversation),
        )
}

pub fn public_router() -> Router {
    Router::new().route(
        "/api/agent/chat/task-runner/callback",
        post(task_runner_callback),
    )
}

async fn agent_chat_send(
    auth: AuthUser,
    Json(mut req): Json<ChatStreamRequest>,
) -> Result<(StatusCode, Json<Value>), (StatusCode, Json<Value>)> {
    ensure_and_set_user_id(&mut req.user_id, &auth)?;
    validate_chat_stream_request(&req, false).await?;
    let conversation_id = req.conversation_id.clone().unwrap_or_default();
    let accepted_turn_id = normalize_turn_id(req.turn_id.as_deref());
    let user_message_id = Uuid::new_v4().to_string();
    req.user_message_id = Some(user_message_id.clone());

    abort_registry::reset_turn(&conversation_id, accepted_turn_id.as_deref());
    if let Some(turn_id) = accepted_turn_id.as_deref() {
        guidance::register_active_turn(&conversation_id, turn_id);
    }
    access_token_scope::spawn_with_current_access_token(stream_chat(None, req));

    Ok((
        StatusCode::ACCEPTED,
        Json(json!({
            "accepted": true,
            "conversation_id": conversation_id,
            "turn_id": accepted_turn_id,
            "user_message_id": user_message_id,
            "source_user_message_id": user_message_id,
        })),
    ))
}

async fn agent_chat_guidance(
    auth: AuthUser,
    Json(req): Json<RuntimeGuidanceRequest>,
) -> (StatusCode, Json<Value>) {
    let conversation_id = req.conversation_id.unwrap_or_default().trim().to_string();
    if conversation_id.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({
                "accepted": false,
                "code": "missing_conversation_id",
                "error": "缺少 conversation_id",
            })),
        );
    }

    let turn_id = match normalize_turn_id(req.turn_id.as_deref()) {
        Some(value) => value,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({
                    "accepted": false,
                    "code": "missing_turn_id",
                    "error": "缺少 turn_id",
                })),
            );
        }
    };

    let content = req.content.unwrap_or_default().trim().to_string();
    let raw_attachments = req.attachments.unwrap_or_default();
    let guidance_attachments = attachments::parse_attachments(raw_attachments.as_slice());
    if content.is_empty() && guidance_attachments.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({
                "accepted": false,
                "code": "empty_guidance",
                "error": "追加指令内容不能为空",
            })),
        );
    }

    let session = match ensure_owned_session(&conversation_id, &auth).await {
        Ok(session) => session,
        Err(err) => return map_session_access_error(err),
    };

    let guidance_item = match guidance::enqueue_runtime_guidance_with_attachments(
        conversation_id.as_str(),
        turn_id.as_str(),
        content.as_str(),
        guidance_attachments.clone(),
    ) {
        Ok(item) => item,
        Err(guidance::EnqueueGuidanceError::TurnNotRunning) => {
            return (
                StatusCode::CONFLICT,
                Json(json!({
                    "accepted": false,
                    "code": "turn_not_running",
                    "error": "目标轮次已结束，无法追加指令",
                    "conversation_id": conversation_id,
                    "turn_id": turn_id,
                })),
            );
        }
    };

    let mut metadata = serde_json::Map::new();
    metadata.insert(
        "conversation_turn_id".to_string(),
        Value::String(turn_id.clone()),
    );
    metadata.insert(
        "runtime_guidance".to_string(),
        json!({
            "guidance_id": guidance_item.guidance_id.clone(),
            "target_turn_id": turn_id.clone(),
            "status": guidance_item.status.clone(),
            "created_at": guidance_item.created_at.clone(),
        }),
    );
    let sanitized_attachments = attachments::sanitize_attachments_for_db(&guidance_attachments);
    if !sanitized_attachments.is_empty() {
        metadata.insert(
            "attachments".to_string(),
            Value::Array(sanitized_attachments),
        );
    }

    let message = build_message(
        conversation_id.clone(),
        NewMessageFields {
            role: Some("user".to_string()),
            content: Some(content),
            message_mode: Some("runtime_guidance".to_string()),
            message_source: Some("runtime_guidance".to_string()),
            metadata: Some(Value::Object(metadata)),
            ..NewMessageFields::default()
        },
        "user",
    );

    let saved = match chatos_sessions::upsert_message_in_session(&session, &message).await {
        Ok(message) => message,
        Err(err) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({
                    "accepted": false,
                    "error": "保存追加指令消息失败",
                    "detail": err,
                })),
            );
        }
    };

    (
        StatusCode::ACCEPTED,
        Json(json!({
            "accepted": true,
            "conversation_id": conversation_id,
            "turn_id": guidance_item.turn_id.clone(),
            "guidance": guidance_item,
            "message": MessageOut::from(saved),
        })),
    )
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
    let turn_id = normalize_turn_id(req.get("turn_id").and_then(Value::as_str));
    if conversation_id.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"success": false, "message": "缺少 conversation_id"})),
        );
    }
    let ok = abort_registry::abort_turn(conversation_id.as_str(), turn_id.as_deref());
    if ok {
        return (
            StatusCode::OK,
            Json(json!({
                "success": true,
                "message": "停止中",
                "conversation_id": conversation_id,
                "turn_id": turn_id,
            })),
        );
    }
    (
        StatusCode::OK,
        Json(json!({
            "success": false,
            "message": if turn_id.is_some() {
                "当前轮次已切换，停止请求已忽略"
            } else {
                "未找到可停止的对话线程或已停止"
            },
            "conversation_id": conversation_id,
            "turn_id": turn_id,
        })),
    )
}

async fn stream_chat(sender: Option<SseSender>, req: ChatStreamRequest) {
    run_chat_usecase(RunChatUsecaseInput { sender, req }).await;
}
