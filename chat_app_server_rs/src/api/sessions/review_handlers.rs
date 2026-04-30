use axum::{
    extract::Path,
    http::StatusCode,
    Json,
};
use serde_json::{json, Value};
use tokio::time::{sleep, Duration};

use crate::core::auth::AuthUser;
use crate::core::session_access::{ensure_owned_session, map_session_access_error_with_success};
use crate::services::memory_server_client;
use crate::services::realtime::{
    publish_review_repair_completed, publish_review_repair_failed, publish_review_repair_progress,
    publish_review_repair_started, user_has_realtime_listeners,
};

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
    let user_id = session.user_id.clone().unwrap_or_else(|| auth.user_id.clone());

    match memory_server_client::run_review_repair_summary(&req).await {
        Ok(result) => {
            let response = (
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
            );

            publish_review_repair_started(user_id.as_str(), &conversation_id, &req, &result);
            if user_has_realtime_listeners() {
                spawn_review_repair_status_bridge(
                    user_id,
                    conversation_id,
                    req,
                    Some(result.pending_message_count),
                );
            }

            response
        }
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

fn spawn_review_repair_status_bridge(
    user_id: String,
    conversation_id: String,
    req: memory_server_client::RunReviewRepairSummaryRequestDto,
    initial_pending_count: Option<i64>,
) {
    memory_server_client::spawn_with_current_access_token(async move {
        let mut last_running: Option<bool> = None;
        let mut last_pending_count: Option<i64> = initial_pending_count;
        let mut stable_non_running_count = 0_u8;

        loop {
            match memory_server_client::get_review_repair_status(&req).await {
                Ok(status) => {
                    let should_publish = last_running != Some(status.running)
                        || last_pending_count != Some(status.pending_message_count);
                    if should_publish {
                        publish_review_repair_progress(
                            &user_id,
                            &conversation_id,
                            &req,
                            &status,
                        );
                        last_running = Some(status.running);
                        last_pending_count = Some(status.pending_message_count);
                    }

                    if status.running {
                        stable_non_running_count = 0;
                        sleep(Duration::from_millis(1200)).await;
                        continue;
                    }

                    stable_non_running_count = stable_non_running_count.saturating_add(1);
                    if stable_non_running_count >= 2 {
                        publish_review_repair_completed(
                            &user_id,
                            &conversation_id,
                            &req,
                            &status,
                        );
                        break;
                    }

                    sleep(Duration::from_millis(900)).await;
                }
                Err(err) => {
                    publish_review_repair_failed(
                        &user_id,
                        &conversation_id,
                        &req,
                        last_pending_count,
                        err.as_str(),
                    );
                    break;
                }
            }
        }
    });
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
