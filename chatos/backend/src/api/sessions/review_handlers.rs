// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::{extract::Path, http::StatusCode, Json};
use serde_json::{json, Value};

use crate::core::auth::AuthUser;
use crate::core::session_access::{ensure_owned_session, map_session_access_error_with_success};
use crate::modules::conversation_runtime::review_repair::{
    get_review_repair_status, run_session_review_repair as run_conversation_review_repair,
    RunSessionReviewRepairResult,
};

pub(super) async fn run_session_review_repair(
    auth: AuthUser,
    Path(conversation_id): Path<String>,
) -> (StatusCode, Json<Value>) {
    let session = match ensure_owned_session(&conversation_id, &auth).await {
        Ok(session) => session,
        Err(err) => return map_session_access_error_with_success(err),
    };

    match run_conversation_review_repair(session, auth.user_id.as_str()).await {
        RunSessionReviewRepairResult::NoPending(scope) => (
            StatusCode::OK,
            Json(json!({
                "success": false,
                "conversation_id": conversation_id,
                "conversationId": conversation_id,
                "project_id": scope.project_id,
                "contact_id": scope.contact_id,
                "agent_id": scope.agent_id,
                "error": "当前没有可复盘的内容",
                "detail": "当前会话里没有未被总结的消息，无需执行复盘"
            })),
        ),
        RunSessionReviewRepairResult::Queued(scope) => (
            StatusCode::ACCEPTED,
            Json(json!({
                "accepted": true,
                "success": true,
                "conversation_id": conversation_id,
                "conversationId": conversation_id,
                "project_id": scope.project_id,
                "contact_id": scope.contact_id,
                "agent_id": scope.agent_id,
                "running": true,
                "queued": true
            })),
        ),
    }
}

pub(super) async fn get_session_review_repair_status(
    auth: AuthUser,
    Path(conversation_id): Path<String>,
) -> (StatusCode, Json<Value>) {
    let session = match ensure_owned_session(&conversation_id, &auth).await {
        Ok(session) => session,
        Err(err) => return map_session_access_error_with_success(err),
    };

    match get_review_repair_status(&session).await {
        Ok(result) => (
            StatusCode::OK,
            Json(json!({
                "success": true,
                "conversation_id": conversation_id,
                "conversationId": conversation_id,
                "project_id": result.project_id,
                "contact_id": result.contact_id,
                "agent_id": result.agent_id,
                "result": result
            })),
        ),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({
                "success": false,
                "error": "获取复盘状态失败",
                "detail": err
            })),
        ),
    }
}
