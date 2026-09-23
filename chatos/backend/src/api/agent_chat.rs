// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

mod task_runner_callback;
#[path = "agent_chat/tools_panel.rs"]
mod tools_panel;

use axum::http::{HeaderMap, StatusCode};
use axum::{
    extract::Path,
    routing::{get, post},
    Extension, Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use self::task_runner_callback::task_runner_callback;
use self::tools_panel::{agent_status, agent_tools};
use crate::api::chat_stream_common::{validate_chat_stream_request, ChatStreamRequest};
use crate::api::conversation_semantics::extract_conversation_scope_id;
use crate::api::RequestClientScopes;
use crate::core::auth::{access_token_from_headers, AuthUser};
use crate::core::messages::{build_message, MessageOut, NewMessageFields};
use crate::core::session_access::{ensure_owned_session, map_session_access_error};
use crate::core::session_project_scope::ensure_session_project_scope;
use crate::core::user_scope::ensure_and_set_user_id;
use crate::modules::conversation_runtime::chat_usecase::{run_chat_usecase, RunChatUsecaseInput};
use crate::modules::conversation_runtime::guidance;
use crate::modules::conversation_runtime::messages as conversation_messages;
use crate::services::access_token_scope;
use crate::services::ai_common::normalize_turn_id;
use crate::services::chatos_sessions;
use crate::services::runtime_guidance_manager::{runtime_guidance_manager, AdmitTurnResult};
use crate::utils::abort_registry;
use crate::utils::attachments;
use crate::utils::sse::SseSender;

const WECHAT_COMPANION_MESSAGE_MAX_CHARS: usize = 20_000;
const IDEMPOTENCY_KEY_HEADER: &str = "idempotency-key";
const IDEMPOTENCY_KEY_MAX_CHARS: usize = 128;

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

pub fn internal_router() -> Router {
    Router::new().route(
        "/api/agent/chat/task-runner/callback",
        post(task_runner_callback),
    )
}

async fn agent_chat_send(
    auth: AuthUser,
    client_scopes: Option<Extension<RequestClientScopes>>,
    headers: HeaderMap,
    Json(mut req): Json<ChatStreamRequest>,
) -> Result<(StatusCode, Json<Value>), (StatusCode, Json<Value>)> {
    // The chat continues after the HTTP response is returned. Capture the
    // already-authenticated request token explicitly instead of relying on an
    // ambient task-local surviving Axum's detached execution boundary.
    let request_access_token =
        access_token_from_headers(&headers).map_err(|err| err.into_response())?;
    if client_scopes
        .as_ref()
        .is_some_and(|Extension(scopes)| scopes.is_wechat_companion())
    {
        validate_wechat_companion_chat_request(&req)?;
    }
    ensure_and_set_user_id(&mut req.user_id, &auth)?;
    req.user_role = Some(auth.role.clone());
    validate_chat_stream_request(&req, false).await?;
    let conversation_id = req.conversation_id.clone().unwrap_or_default();
    let session = ensure_owned_session(conversation_id.as_str(), &auth)
        .await
        .map_err(map_session_access_error)?;
    ensure_session_project_scope(&session, req.project_id.as_deref())?;
    let accepted_turn_id =
        normalize_turn_id(req.turn_id.as_deref()).unwrap_or_else(|| Uuid::new_v4().to_string());
    req.turn_id = Some(accepted_turn_id.clone());
    let idempotency_key = resolve_idempotency_key(&headers, accepted_turn_id.as_str())?;
    let request_fingerprint = chat_request_fingerprint(&req)?;
    let user_message_id = Uuid::new_v4().to_string();

    match runtime_guidance_manager().admit_turn(
        conversation_id.as_str(),
        accepted_turn_id.as_str(),
        user_message_id.as_str(),
        idempotency_key.as_str(),
        request_fingerprint.as_str(),
    ) {
        AdmitTurnResult::Accepted => {}
        AdmitTurnResult::Duplicate {
            turn_id,
            user_message_id,
        } => {
            return Ok((
                StatusCode::ACCEPTED,
                Json(json!({
                    "accepted": true,
                    "duplicate": true,
                    "conversation_id": conversation_id,
                    "turn_id": turn_id,
                    "user_message_id": user_message_id,
                    "source_user_message_id": user_message_id,
                })),
            ));
        }
        AdmitTurnResult::ActiveTurnConflict { active_turn_id } => {
            return Err((
                StatusCode::CONFLICT,
                Json(json!({
                    "accepted": false,
                    "code": "conversation_turn_active",
                    "error": "This conversation already has an active turn; send guidance or stop it first",
                    "conversation_id": conversation_id,
                    "active_turn_id": active_turn_id,
                })),
            ));
        }
        AdmitTurnResult::IdempotencyConflict => {
            return Err((
                StatusCode::CONFLICT,
                Json(json!({
                    "accepted": false,
                    "code": "idempotency_key_reused",
                    "error": "The idempotency key was already used for a different chat request",
                    "conversation_id": conversation_id,
                })),
            ));
        }
    }

    req.user_message_id = Some(user_message_id.clone());

    abort_registry::reset_turn(&conversation_id, Some(accepted_turn_id.as_str()));
    access_token_scope::spawn_with_access_token(Some(request_access_token), stream_chat(None, req));

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

fn resolve_idempotency_key(
    headers: &HeaderMap,
    fallback_turn_id: &str,
) -> Result<String, (StatusCode, Json<Value>)> {
    let Some(value) = headers.get(IDEMPOTENCY_KEY_HEADER) else {
        return Ok(fallback_turn_id.to_string());
    };
    let value = value.to_str().map_err(|_| {
        (
            StatusCode::BAD_REQUEST,
            Json(json!({
                "error": "Idempotency-Key must be valid ASCII text",
                "code": "invalid_idempotency_key"
            })),
        )
    })?;
    let value = value.trim();
    if value.is_empty()
        || value.chars().count() > IDEMPOTENCY_KEY_MAX_CHARS
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':'))
    {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({
                "error": "Idempotency-Key must contain 1-128 letters, digits, '-', '_', '.' or ':'",
                "code": "invalid_idempotency_key"
            })),
        ));
    }
    Ok(value.to_string())
}

fn chat_request_fingerprint(req: &ChatStreamRequest) -> Result<String, (StatusCode, Json<Value>)> {
    let encoded = serde_json::to_vec(req).map_err(|error| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({
                "error": format!("Failed to fingerprint chat request: {error}"),
                "code": "chat_request_fingerprint_failed"
            })),
        )
    })?;
    Ok(hex::encode(Sha256::digest(encoded)))
}

fn validate_wechat_companion_chat_request(
    req: &ChatStreamRequest,
) -> Result<(), (StatusCode, Json<Value>)> {
    let overrides_runtime = req.model_config_id.is_some()
        || req.ai_model_config.is_some()
        || req.user_id.is_some()
        || req.reasoning_enabled.is_some()
        || req.contact_agent_id.is_some()
        || req.project_id.is_some()
        || req.project_root.is_some()
        || req.workspace_root.is_some()
        || req.remote_connection_id.is_some()
        || !req.task_plugin_preferences.is_empty()
        || req.unsupported_plugin_agent_selection.is_some()
        || req.attachments.is_some();
    if overrides_runtime {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({
                "error": "WeChat Companion must use the conversation's saved runtime context",
                "code": "companion_runtime_override_forbidden"
            })),
        ));
    }
    validate_wechat_companion_message_content(req.content.as_deref())?;
    Ok(())
}

fn validate_wechat_companion_message_content(
    content: Option<&str>,
) -> Result<(), (StatusCode, Json<Value>)> {
    if content.is_some_and(|content| content.chars().count() > WECHAT_COMPANION_MESSAGE_MAX_CHARS) {
        return Err((
            StatusCode::PAYLOAD_TOO_LARGE,
            Json(json!({
                "error": "WeChat Companion messages are limited to 20,000 characters",
                "code": "companion_message_too_large"
            })),
        ));
    }
    Ok(())
}

async fn agent_chat_guidance(
    auth: AuthUser,
    client_scopes: Option<Extension<RequestClientScopes>>,
    Json(req): Json<RuntimeGuidanceRequest>,
) -> (StatusCode, Json<Value>) {
    let companion = client_scopes
        .as_ref()
        .is_some_and(|Extension(scopes)| scopes.is_wechat_companion());
    if companion {
        if req.attachments.is_some() {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({
                    "accepted": false,
                    "code": "companion_attachments_forbidden",
                    "error": "微信伴侣端不允许发送附件",
                })),
            );
        }
        if let Err(error) = validate_wechat_companion_message_content(req.content.as_deref()) {
            return error;
        }
    }
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
    if let Err(error) = ensure_session_project_scope(&session, None) {
        return error;
    }

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

    let response = if companion {
        json!({
            "accepted": true,
            "conversation_id": conversation_id,
            "turn_id": guidance_item.turn_id,
            "guidance_id": guidance_item.guidance_id,
            "message_id": saved.id,
        })
    } else {
        json!({
            "accepted": true,
            "conversation_id": conversation_id,
            "turn_id": guidance_item.turn_id.clone(),
            "guidance": guidance_item,
            "message": MessageOut::from(saved),
        })
    };
    (StatusCode::ACCEPTED, Json(response))
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

async fn stop_chat(auth: AuthUser, Json(req): Json<Value>) -> (StatusCode, Json<Value>) {
    let conversation_id = extract_conversation_scope_id(&req).unwrap_or_default();
    let turn_id = normalize_turn_id(req.get("turn_id").and_then(Value::as_str));
    if conversation_id.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"success": false, "message": "缺少 conversation_id"})),
        );
    }
    if let Err(err) = ensure_owned_session(conversation_id.as_str(), &auth).await {
        return map_session_access_error(err);
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
    run_chat_usecase(RunChatUsecaseInput {
        sender,
        req,
        persisted_user_message_content: None,
        persisted_user_message_metadata: None,
        cloud_agent_owner_context: None,
    })
    .await;
}

#[cfg(test)]
mod tests {
    use super::{
        validate_wechat_companion_chat_request, validate_wechat_companion_message_content,
        ChatStreamRequest, WECHAT_COMPANION_MESSAGE_MAX_CHARS,
    };
    use axum::http::StatusCode;
    use serde_json::json;

    fn request(value: serde_json::Value) -> ChatStreamRequest {
        serde_json::from_value(value).expect("valid chat request")
    }

    #[test]
    fn companion_chat_accepts_only_message_and_turn_identity() {
        let valid = request(json!({
            "conversation_id": "conversation-1",
            "content": "continue",
            "turn_id": "turn-1"
        }));
        assert!(validate_wechat_companion_chat_request(&valid).is_ok());

        for override_payload in [
            json!({ "model_config_id": "model-1" }),
            json!({ "ai_model_config": { "api_key": "secret" } }),
            json!({ "user_id": "user-2" }),
            json!({ "reasoning_enabled": true }),
            json!({ "contact_agent_id": "agent-1" }),
            json!({ "project_id": "project-1" }),
            json!({ "project_root": "/private/project" }),
            json!({ "workspace_root": "/private/workspace" }),
            json!({ "remote_connection_id": "remote-1" }),
            json!({ "task_plugin_preferences": ["plugin-1"] }),
            json!({ "plugin_agent_selection": { "agent": "agent-1" } }),
            json!({ "attachments": [] }),
        ] {
            let mut payload = json!({
                "conversation_id": "conversation-1",
                "content": "continue"
            });
            payload.as_object_mut().expect("object").extend(
                override_payload
                    .as_object()
                    .expect("override object")
                    .clone(),
            );
            assert!(
                validate_wechat_companion_chat_request(&request(payload)).is_err(),
                "override must be rejected"
            );
        }
    }

    #[test]
    fn companion_chat_rejects_oversized_messages() {
        let oversized = "a".repeat(WECHAT_COMPANION_MESSAGE_MAX_CHARS + 1);
        let error = validate_wechat_companion_message_content(Some(oversized.as_str()))
            .expect_err("oversized message must be rejected");
        assert_eq!(error.0, StatusCode::PAYLOAD_TOO_LARGE);
        assert_eq!(error.1["code"], "companion_message_too_large");
        assert!(validate_wechat_companion_message_content(Some(
            "好".repeat(WECHAT_COMPANION_MESSAGE_MAX_CHARS).as_str()
        ))
        .is_ok());
    }
}
