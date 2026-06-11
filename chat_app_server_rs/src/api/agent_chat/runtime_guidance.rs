use axum::Json;
use axum::http::StatusCode;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::core::auth::AuthUser;
use crate::modules::conversation_runtime::guidance::{
    SubmitRuntimeGuidanceError, SubmitRuntimeGuidanceInput,
    submit_runtime_guidance as submit_runtime_guidance_command,
};

#[derive(Debug, Deserialize)]
pub(super) struct RuntimeGuidanceRequest {
    #[serde(rename = "conversation_id", alias = "conversationId")]
    pub(super) conversation_id: Option<String>,
    pub(super) turn_id: Option<String>,
    pub(super) content: Option<String>,
    pub(super) project_id: Option<String>,
    pub(super) attachments: Option<Vec<Value>>,
}

pub(super) async fn submit_runtime_guidance(
    auth: AuthUser,
    Json(req): Json<RuntimeGuidanceRequest>,
) -> (StatusCode, Json<Value>) {
    match submit_runtime_guidance_command(
        &auth,
        SubmitRuntimeGuidanceInput {
            conversation_id: req.conversation_id,
            turn_id: req.turn_id,
            content: req.content,
            project_id: req.project_id,
            attachments: req.attachments,
        },
    )
    .await
    {
        Ok(outcome) => (
            StatusCode::OK,
            Json(json!({
                "success": true,
                "conversation_id": outcome.conversation_id,
                "guidance_id": outcome.guidance_id,
                "status": "queued",
                "pending_count": outcome.pending_count,
                "turn_id": outcome.turn_id,
            })),
        ),
        Err(SubmitRuntimeGuidanceError::InvalidPayload) => (
            StatusCode::BAD_REQUEST,
            Json(json!({
                "success": false,
                "error": "conversation_id / turn_id 不能为空，且 content / attachments 至少要提供一项",
                "code": "invalid_runtime_guidance_payload",
            })),
        ),
        Err(SubmitRuntimeGuidanceError::TooLong { max_length }) => (
            StatusCode::BAD_REQUEST,
            Json(json!({
                "success": false,
                "error": format!("content 长度不能超过 {} 字符", max_length),
                "code": "runtime_guidance_too_long",
                "max_length": max_length,
            })),
        ),
        Err(SubmitRuntimeGuidanceError::SessionNotFound) => (
            StatusCode::NOT_FOUND,
            Json(json!({
                "success": false,
                "error": "对话线程不存在",
                "code": "session_not_found",
            })),
        ),
        Err(SubmitRuntimeGuidanceError::Forbidden) => (
            StatusCode::FORBIDDEN,
            Json(json!({
                "success": false,
                "error": "对话线程不属于当前用户",
                "code": "user_scope_forbidden",
            })),
        ),
        Err(SubmitRuntimeGuidanceError::SessionLookupFailed) => (
            StatusCode::BAD_GATEWAY,
            Json(json!({
                "success": false,
                "error": "查询对话线程失败",
                "code": "session_lookup_failed",
            })),
        ),
        Err(SubmitRuntimeGuidanceError::ProjectScopeMismatch {
            requested_project_id,
            session_project_id,
        }) => (
            StatusCode::CONFLICT,
            Json(json!({
                "success": false,
                "error": "对话线程项目不匹配，已阻止跨项目引导",
                "code": "project_scope_mismatch",
                "session_project_id": session_project_id,
                "requested_project_id": requested_project_id,
            })),
        ),
        Err(SubmitRuntimeGuidanceError::TurnNotRunning) => (
            StatusCode::CONFLICT,
            Json(json!({
                "success": false,
                "error": "当前轮次未运行或已结束",
                "code": "turn_not_running",
            })),
        ),
    }
}
