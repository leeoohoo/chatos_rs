use axum::{
    extract::Path,
    http::StatusCode,
    Json,
};
use serde_json::{json, Value};

use crate::core::auth::AuthUser;
use crate::core::session_access::{ensure_owned_session, map_session_access_error_with_success};
use crate::services::memory_server_client;
use crate::services::realtime::{
    publish_review_repair_completed, publish_review_repair_failed,
    publish_review_repair_started_pending, user_has_realtime_listeners,
};

use super::support::{
    contact_agent_id_from_metadata, contact_id_from_metadata, resolve_session_project_scope,
};

fn build_review_repair_scope_request(session: &crate::models::session::Session) -> Result<
    memory_server_client::RunReviewRepairSummaryRequestDto,
    (StatusCode, Json<Value>),
> {
    let metadata = session.metadata.as_ref();
    let project_id = resolve_session_project_scope(session.project_id.as_deref(), metadata);
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
    let user_id = session.user_id.clone().unwrap_or_else(|| auth.user_id.clone());

    let initial_pending_count = memory_server_client::get_review_repair_status(&req)
        .await
        .ok()
        .map(|status| status.pending_message_count);
    publish_review_repair_started_pending(
        user_id.as_str(),
        &conversation_id,
        &req,
        initial_pending_count,
    );

    spawn_review_repair_run(
        user_id,
        conversation_id.clone(),
        req,
        initial_pending_count,
    );

    (
        StatusCode::ACCEPTED,
        Json(json!({
            "accepted": true,
            "success": true,
            "conversation_id": conversation_id,
            "conversationId": conversation_id,
            "project_id": project_id,
            "contact_id": contact_id,
            "agent_id": agent_id,
            "running": true,
            "queued": true
        })),
    )
}

fn spawn_review_repair_run(
    user_id: String,
    conversation_id: String,
    req: memory_server_client::RunReviewRepairSummaryRequestDto,
    initial_pending_count: Option<i64>,
) {
    memory_server_client::spawn_with_current_access_token(async move {
        if !user_has_realtime_listeners() {
            match memory_server_client::run_review_repair_summary(&req).await {
                Ok(result) => {
                    finish_review_repair_success(
                        user_id.as_str(),
                        &conversation_id,
                        &req,
                        &result,
                        initial_pending_count,
                        None,
                    )
                    .await;
                }
                Err(err) => {
                    publish_review_repair_failed(
                        user_id.as_str(),
                        &conversation_id,
                        &req,
                        initial_pending_count,
                        err.as_str(),
                    );
                }
            }
            return;
        }

        let run_future = memory_server_client::run_review_repair_summary(&req);
        tokio::pin!(run_future);
        match run_future.await {
            Ok(result) => {
                finish_review_repair_success(
                    user_id.as_str(),
                    &conversation_id,
                    &req,
                    &result,
                    initial_pending_count,
                    None,
                ).await;
            }
            Err(err) => {
                publish_review_repair_failed(
                    user_id.as_str(),
                    &conversation_id,
                    &req,
                    initial_pending_count,
                    err.as_str(),
                );
            }
        }
    });
}

async fn finish_review_repair_success(
    user_id: &str,
    conversation_id: &str,
    req: &memory_server_client::RunReviewRepairSummaryRequestDto,
    result: &memory_server_client::ReviewRepairSummaryRunResultDto,
    initial_pending_count: Option<i64>,
    final_status_candidate: Option<memory_server_client::ReviewRepairStatusDto>,
) {
    let final_status = match memory_server_client::get_review_repair_status(req).await {
        Ok(status) => status,
        Err(_) => final_status_candidate.unwrap_or_else(|| {
            build_review_repair_completed_status_fallback(req, result, initial_pending_count)
        }),
    };

    publish_review_repair_completed(
        user_id,
        conversation_id,
        req,
        &final_status,
    );
}

fn build_review_repair_completed_status_fallback(
    req: &memory_server_client::RunReviewRepairSummaryRequestDto,
    result: &memory_server_client::ReviewRepairSummaryRunResultDto,
    initial_pending_count: Option<i64>,
) -> memory_server_client::ReviewRepairStatusDto {
    let base_pending_count = initial_pending_count.unwrap_or(result.pending_message_count);
    let pending_message_count = base_pending_count.saturating_sub(result.marked_messages as i64);

    memory_server_client::ReviewRepairStatusDto {
        running: false,
        running_job_count: 0,
        pending_message_count,
        scope_session_count: result.processed_sessions,
        project_id: result.project_id.clone(),
        contact_id: result.contact_id.clone().or_else(|| req.contact_id.clone()),
        agent_id: result.agent_id.clone().or_else(|| req.agent_id.clone()),
        job_type: "summary_review_repair".to_string(),
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
