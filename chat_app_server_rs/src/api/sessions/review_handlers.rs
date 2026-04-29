use axum::{
    extract::Path,
    http::StatusCode,
    Json,
};
use serde_json::{json, Value};

use crate::core::auth::AuthUser;
use crate::core::session_access::{ensure_owned_session, map_session_access_error_with_success};
use crate::services::memory_server_client;

use super::support::{contact_agent_id_from_metadata, contact_id_from_metadata, normalize_project_scope};

fn build_review_repair_scope_request(session: &crate::models::session::Session) -> Result<
    memory_server_client::RunReviewRepairSummaryRequestDto,
    (StatusCode, Json<Value>),
> {
    let metadata = session.metadata.as_ref();
    let project_id = normalize_project_scope(session.project_id.as_deref());
    let contact_id = contact_id_from_metadata(metadata);
    let agent_id = contact_agent_id_from_metadata(metadata);

    if contact_id.is_none() && agent_id.is_none() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({
                "success": false,
                "error": "当前会话没有可用的联系人或智能体上下文，无法执行复盘"
            })),
        ));
    }

    Ok(memory_server_client::RunReviewRepairSummaryRequestDto {
        user_id: session.user_id.clone(),
        project_id: Some(project_id),
        contact_id,
        agent_id,
    })
}

pub(super) async fn run_session_review_repair(
    auth: AuthUser,
    Path(conversation_id): Path<String>,
) -> (StatusCode, Json<Value>) {
    let session = match ensure_owned_session(&conversation_id, &auth).await {
        Ok(session) => session,
        Err(err) => return map_session_access_error_with_success(err),
    };

    let req = match build_review_repair_scope_request(&session) {
        Ok(value) => value,
        Err(err) => return err,
    };
    let project_id = req.project_id.clone().unwrap_or_else(|| "0".to_string());
    let contact_id = req.contact_id.clone();
    let agent_id = req.agent_id.clone();

    match memory_server_client::run_review_repair_summary(&req).await {
        Ok(result) => (
            StatusCode::OK,
            Json(json!({
                "success": true,
                "conversation_id": conversation_id,
                "conversationId": conversation_id,
                "project_id": project_id,
                "contact_id": contact_id,
                "agent_id": agent_id,
                "result": result
            })),
        ),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({
                "success": false,
                "error": "执行复盘失败",
                "detail": err
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

    let req = match build_review_repair_scope_request(&session) {
        Ok(value) => value,
        Err(err) => return err,
    };
    let project_id = req.project_id.clone().unwrap_or_else(|| "0".to_string());
    let contact_id = req.contact_id.clone();
    let agent_id = req.agent_id.clone();

    match memory_server_client::get_review_repair_status(&req).await {
        Ok(result) => (
            StatusCode::OK,
            Json(json!({
                "success": true,
                "conversation_id": conversation_id,
                "conversationId": conversation_id,
                "project_id": project_id,
                "contact_id": contact_id,
                "agent_id": agent_id,
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
