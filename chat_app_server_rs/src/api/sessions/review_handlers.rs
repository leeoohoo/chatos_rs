use axum::{
    extract::Path,
    http::StatusCode,
    Json,
};
use serde_json::{json, Value};

use crate::core::auth::AuthUser;
use crate::core::session_access::{ensure_owned_session, map_session_access_error_with_success};
use crate::models::memory_runtime_types::{ReviewRepairStatusDto, RunReviewRepairSummaryRequestDto};
use crate::services::access_token_scope;
use crate::services::{chatos_memory_engine, chatos_sessions};
use crate::services::realtime::{
    publish_conversation_summaries_updated,
    publish_review_repair_completed, publish_review_repair_failed,
    publish_review_repair_started_pending, user_has_realtime_listeners,
};

use super::support::{
    contact_agent_id_from_metadata, contact_id_from_metadata, resolve_session_project_scope,
};

pub(super) async fn run_session_review_repair(
    auth: AuthUser,
    Path(conversation_id): Path<String>,
) -> (StatusCode, Json<Value>) {
    let session = match ensure_owned_session(&conversation_id, &auth).await {
        Ok(session) => session,
        Err(err) => return map_session_access_error_with_success(err),
    };

    let review_req = chatos_memory_engine::ChatosReviewRepairRequest {
        session: session.clone(),
    };
    let scope = match chatos_memory_engine::get_chatos_review_repair_status(&review_req).await {
        Ok(value) => value,
        Err(_) => chatos_memory_engine::ReviewRepairStatusResult {
            running: false,
            running_job_count: 0,
            pending_message_count: 0,
            scope_session_count: 0,
            project_id: resolve_session_project_scope(session.project_id.as_deref(), session.metadata.as_ref()),
            contact_id: contact_id_from_metadata(session.metadata.as_ref()),
            agent_id: contact_agent_id_from_metadata(session.metadata.as_ref()),
            job_type: "review_repair".to_string(),
        },
    };
    let project_id = scope.project_id.clone();
    let contact_id = scope.contact_id.clone();
    let agent_id = scope.agent_id.clone();
    let user_id = session.user_id.clone().unwrap_or_else(|| auth.user_id.clone());

    if scope.pending_message_count <= 0 {
        return (
            StatusCode::OK,
            Json(json!({
                "success": false,
                "conversation_id": conversation_id,
                "conversationId": conversation_id,
                "project_id": project_id,
                "contact_id": contact_id,
                "agent_id": agent_id,
                "error": "当前没有可复盘的内容",
                "detail": "当前会话里没有未被总结的消息，无需执行复盘"
            })),
        );
    }

    publish_review_repair_started_pending(
        user_id.as_str(),
        &conversation_id,
        &RunReviewRepairSummaryRequestDto {
            user_id: session.user_id.clone(),
            project_id: Some(project_id.clone()),
            contact_id: contact_id.clone(),
            agent_id: agent_id.clone(),
        },
        Some(scope.pending_message_count),
    );

    spawn_review_repair_run(
        user_id,
        conversation_id.clone(),
        review_req,
        Some(scope.pending_message_count),
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

fn build_compat_review_req(
    session: &crate::models::session::Session,
) -> RunReviewRepairSummaryRequestDto {
    let metadata = session.metadata.as_ref();
    RunReviewRepairSummaryRequestDto {
        user_id: session.user_id.clone(),
        project_id: Some(resolve_session_project_scope(
            session.project_id.as_deref(),
            metadata,
        )),
        contact_id: contact_id_from_metadata(metadata),
        agent_id: contact_agent_id_from_metadata(metadata),
    }
}

fn spawn_review_repair_run(
    user_id: String,
    conversation_id: String,
    req: chatos_memory_engine::ChatosReviewRepairRequest,
    initial_pending_count: Option<i64>,
) {
    access_token_scope::spawn_with_current_access_token(async move {
        let compat_req = build_compat_review_req(&req.session);
        if !user_has_realtime_listeners() {
            match chatos_memory_engine::run_chatos_review_repair(&req).await {
                Ok(result) => {
                    finish_review_repair_success(
                        user_id.as_str(),
                        &conversation_id,
                        &compat_req,
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
                        &compat_req,
                        initial_pending_count,
                        err.as_str(),
                    );
                }
            }
            return;
        }

        match chatos_memory_engine::run_chatos_review_repair(&req).await {
            Ok(result) => {
                finish_review_repair_success(
                    user_id.as_str(),
                    &conversation_id,
                    &compat_req,
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
                    &compat_req,
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
    req: &RunReviewRepairSummaryRequestDto,
    result: &chatos_memory_engine::ReviewRepairSummaryRunResult,
    initial_pending_count: Option<i64>,
    final_status_candidate: Option<chatos_memory_engine::ReviewRepairStatusResult>,
) {
    let final_status = match chatos_sessions::get_session_by_id(conversation_id).await {
        Ok(Some(session)) => {
            match chatos_memory_engine::get_chatos_review_repair_status(
                &chatos_memory_engine::ChatosReviewRepairRequest { session }
            ).await {
                Ok(status) => status,
                Err(_) => final_status_candidate.unwrap_or_else(|| {
                    build_review_repair_completed_status_fallback(req, result, initial_pending_count)
                }),
            }
        }
        _ => final_status_candidate.unwrap_or_else(|| {
            build_review_repair_completed_status_fallback(req, result, initial_pending_count)
        }),
    };

    publish_review_repair_completed(
        user_id,
        conversation_id,
        req,
        &ReviewRepairStatusDto {
            running: final_status.running,
            running_job_count: final_status.running_job_count,
            pending_message_count: final_status.pending_message_count,
            scope_session_count: final_status.scope_session_count,
            project_id: final_status.project_id.clone(),
            contact_id: final_status.contact_id.clone(),
            agent_id: final_status.agent_id.clone(),
            job_type: final_status.job_type.clone(),
        },
    );

    if let Ok(items) = chatos_sessions::list_summaries(conversation_id, Some(200), 0).await {
        publish_conversation_summaries_updated(
            user_id,
            conversation_id,
            final_status.project_id.as_str(),
            final_status.contact_id.as_deref(),
            final_status.agent_id.as_deref(),
            items,
            "review_repair_completed",
        );
    }
}

fn build_review_repair_completed_status_fallback(
    req: &RunReviewRepairSummaryRequestDto,
    result: &chatos_memory_engine::ReviewRepairSummaryRunResult,
    initial_pending_count: Option<i64>,
) -> chatos_memory_engine::ReviewRepairStatusResult {
    let base_pending_count = initial_pending_count.unwrap_or(result.pending_message_count);
    let pending_message_count = base_pending_count.saturating_sub(result.marked_messages as i64);

    chatos_memory_engine::ReviewRepairStatusResult {
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

    let req = chatos_memory_engine::ChatosReviewRepairRequest {
        session: session.clone(),
    };

    match chatos_memory_engine::get_chatos_review_repair_status(&req).await {
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
