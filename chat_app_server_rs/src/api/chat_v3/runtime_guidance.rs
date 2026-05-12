use axum::http::StatusCode;
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};
use tracing::warn;

use crate::core::auth::AuthUser;
use crate::core::chat_runtime::project_id_from_metadata;
use crate::core::user_scope::resolve_user_id;
use crate::services::ai_common::normalize_turn_id;
use crate::services::chatos_sessions;
use crate::services::runtime_guidance_manager::{runtime_guidance_manager, EnqueueGuidanceError};
use crate::services::v3::message_manager::MessageManager;
use crate::utils::abort_registry;

#[derive(Debug, Deserialize)]
pub(super) struct RuntimeGuidanceRequest {
    #[serde(rename = "conversation_id", alias = "conversationId")]
    pub(super) conversation_id: Option<String>,
    pub(super) turn_id: Option<String>,
    pub(super) content: Option<String>,
    pub(super) project_id: Option<String>,
}

pub(super) async fn submit_runtime_guidance(
    auth: AuthUser,
    Json(req): Json<RuntimeGuidanceRequest>,
) -> (StatusCode, Json<Value>) {
    const CONTENT_MAX_LEN: usize = 1000;

    let session_id = req
        .conversation_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("");
    let turn_id = normalize_turn_id(req.turn_id.as_deref()).unwrap_or_default();
    let content = req
        .content
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("");
    let requested_project_id = req
        .project_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);

    if session_id.is_empty() || turn_id.is_empty() || content.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({
                "success": false,
                "error": "conversation_id / turn_id / content 不能为空",
                "code": "invalid_runtime_guidance_payload",
            })),
        );
    }
    if content.chars().count() > CONTENT_MAX_LEN {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({
                "success": false,
                "error": format!("content 长度不能超过 {} 字符", CONTENT_MAX_LEN),
                "code": "runtime_guidance_too_long",
                "max_length": CONTENT_MAX_LEN,
            })),
        );
    }

    let auth_user_id = match resolve_user_id(None, &auth) {
        Ok(user_id) => user_id,
        Err(err) => return err,
    };
    let target_session = match chatos_sessions::get_session_by_id(session_id).await {
        Ok(Some(session)) => session,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({
                    "success": false,
                    "error": "对话线程不存在",
                    "code": "session_not_found",
                })),
            );
        }
        Err(err) => {
            warn!(
                "runtime guidance session lookup failed: session_id={} detail={}",
                session_id, err
            );
            return (
                StatusCode::BAD_GATEWAY,
                Json(json!({
                    "success": false,
                    "error": "查询对话线程失败",
                    "code": "session_lookup_failed",
                })),
            );
        }
    };
    let session_user_id = target_session.user_id.as_deref().unwrap_or_default().trim();
    if session_user_id.is_empty() || session_user_id != auth_user_id {
        return (
            StatusCode::FORBIDDEN,
            Json(json!({
                "success": false,
                "error": "对话线程不属于当前用户",
                "code": "user_scope_forbidden",
            })),
        );
    }
    if let Some(requested_project_id) = requested_project_id.as_deref() {
        let session_project_id = target_session
            .project_id
            .clone()
            .or_else(|| project_id_from_metadata(target_session.metadata.as_ref()));
        let requested_scope = normalize_project_scope_id(Some(requested_project_id));
        let session_scope = normalize_project_scope_id(session_project_id.as_deref());
        if requested_scope != session_scope {
            return (
                StatusCode::CONFLICT,
                Json(json!({
                    "success": false,
                    "error": "对话线程项目不匹配，已阻止跨项目引导",
                    "code": "project_scope_mismatch",
                    "session_project_id": session_scope,
                    "requested_project_id": requested_scope,
                })),
            );
        }
    }

    if abort_registry::is_aborted(session_id) {
        return (
            StatusCode::CONFLICT,
            Json(json!({
                "success": false,
                "error": "当前轮次已停止，不再接收引导",
                "code": "turn_not_running",
            })),
        );
    }

    let enqueue_result = runtime_guidance_manager().enqueue_guidance(session_id, &turn_id, content);
    let guidance_item = match enqueue_result {
        Ok(item) => item,
        Err(EnqueueGuidanceError::TurnNotRunning) => {
            return (
                StatusCode::CONFLICT,
                Json(json!({
                    "success": false,
                    "error": "当前轮次未运行或已结束",
                    "code": "turn_not_running",
                })),
            );
        }
    };

    let pending_count = runtime_guidance_manager().pending_count(session_id, &turn_id);
    let guidance_id = guidance_item.guidance_id.clone();

    let metadata = json!({
        "conversation_turn_id": turn_id,
        "hidden": true,
        "runtime_guidance": {
            "guidance_id": guidance_item.guidance_id,
            "status": "queued",
            "created_at": guidance_item.created_at,
        }
    });
    let message_manager = MessageManager::new();
    if let Err(err) = message_manager
        .save_user_message(
            session_id,
            content,
            Some(guidance_id.clone()),
            Some("runtime_guidance".to_string()),
            Some("runtime_guidance".to_string()),
            Some(metadata),
        )
        .await
    {
        warn!(
            "persist runtime guidance failed: session_id={} turn_id={} guidance_id={} detail={}",
            session_id, turn_id, guidance_id, err
        );
    }

    (
        StatusCode::OK,
        Json(json!({
            "success": true,
            "conversation_id": session_id,
            "guidance_id": guidance_id,
            "status": "queued",
            "pending_count": pending_count,
            "turn_id": turn_id,
        })),
    )
}

fn normalize_project_scope_id(value: Option<&str>) -> String {
    let trimmed = value.unwrap_or_default().trim();
    if trimmed.is_empty() {
        "0".to_string()
    } else {
        trimmed.to_string()
    }
}
