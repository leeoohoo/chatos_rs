// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use tokio::time::{sleep, Duration};
use tracing::warn;

use crate::models::memory_runtime_types::{
    ReviewRepairStatusDto, RunReviewRepairSummaryRequestDto,
};
use crate::models::session::Session;
use crate::services::access_token_scope;
use crate::services::realtime::{
    publish_conversation_summaries_updated, publish_review_repair_completed,
    publish_review_repair_failed, publish_review_repair_started_pending,
};
use crate::services::{chatos_memory_engine, chatos_sessions};

use super::session_scope::{
    contact_agent_id_from_metadata, contact_id_from_metadata, resolve_session_project_scope,
};

const REVIEW_REPAIR_POLL_INTERVAL_MS: u64 = 1500;
const REVIEW_REPAIR_POLL_MAX_ATTEMPTS: usize = 210;

#[derive(Debug, Clone)]
pub struct ReviewRepairScopeState {
    pub project_id: String,
    pub contact_id: Option<String>,
    pub agent_id: Option<String>,
    pub pending_message_count: i64,
}

#[derive(Debug, Clone)]
pub enum RunSessionReviewRepairResult {
    Queued(ReviewRepairScopeState),
    NoPending(ReviewRepairScopeState),
}

pub async fn run_session_review_repair(
    session: Session,
    fallback_user_id: &str,
) -> RunSessionReviewRepairResult {
    let conversation_id = session.id.clone();
    let review_req = chatos_memory_engine::ChatosReviewRepairRequest {
        session: session.clone(),
    };
    let scope = load_review_repair_scope(&review_req).await;
    let state = ReviewRepairScopeState {
        project_id: scope.project_id.clone(),
        contact_id: scope.contact_id.clone(),
        agent_id: scope.agent_id.clone(),
        pending_message_count: scope.pending_message_count,
    };
    let user_id = session
        .user_id
        .clone()
        .unwrap_or_else(|| fallback_user_id.to_string());

    if state.pending_message_count <= 0 {
        return RunSessionReviewRepairResult::NoPending(state);
    }

    publish_review_repair_started_pending(
        user_id.as_str(),
        &conversation_id,
        &build_compat_review_req(&session),
        Some(state.pending_message_count),
    );

    spawn_review_repair_run(
        user_id,
        conversation_id,
        review_req,
        Some(state.pending_message_count),
    );

    RunSessionReviewRepairResult::Queued(state)
}

pub async fn get_review_repair_status(session: &Session) -> Result<ReviewRepairStatusDto, String> {
    let req = chatos_memory_engine::ChatosReviewRepairRequest {
        session: session.clone(),
    };
    let result = chatos_memory_engine::get_chatos_review_repair_status(&req).await?;
    Ok(review_repair_status_dto(&result))
}

async fn load_review_repair_scope(
    req: &chatos_memory_engine::ChatosReviewRepairRequest,
) -> chatos_memory_engine::ReviewRepairStatusResult {
    match chatos_memory_engine::get_chatos_review_repair_status(req).await {
        Ok(value) => value,
        Err(_) => chatos_memory_engine::ReviewRepairStatusResult {
            running: false,
            running_job_count: 0,
            pending_message_count: 0,
            scope_session_count: 0,
            project_id: resolve_session_project_scope(
                req.session.project_id.as_deref(),
                req.session.metadata.as_ref(),
            ),
            contact_id: contact_id_from_metadata(req.session.metadata.as_ref()),
            agent_id: contact_agent_id_from_metadata(req.session.metadata.as_ref()),
            job_type: "review_repair".to_string(),
        },
    }
}

fn build_compat_review_req(session: &Session) -> RunReviewRepairSummaryRequestDto {
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
        match chatos_memory_engine::run_chatos_review_repair(&req).await {
            Ok(result) => match wait_for_review_repair_completion(&req, &result).await {
                Ok(final_status) => {
                    finish_review_repair_success(
                        user_id.as_str(),
                        &conversation_id,
                        &compat_req,
                        &result,
                        initial_pending_count,
                        Some(final_status),
                    )
                    .await;
                }
                Err(err) => {
                    let fallback_pending_count =
                        match chatos_memory_engine::get_chatos_review_repair_status(&req).await {
                            Ok(status) => Some(status.pending_message_count),
                            Err(status_err) => {
                                warn!(
                                    "review repair failed and fallback status refresh also failed for conversation {}: {}",
                                    conversation_id, status_err
                                );
                                initial_pending_count
                            }
                        };
                    publish_review_repair_failed(
                        user_id.as_str(),
                        &conversation_id,
                        &compat_req,
                        fallback_pending_count,
                        err.as_str(),
                    );
                }
            },
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

async fn wait_for_review_repair_completion(
    req: &chatos_memory_engine::ChatosReviewRepairRequest,
    launch_result: &chatos_memory_engine::ReviewRepairSummaryRunResult,
) -> Result<chatos_memory_engine::ReviewRepairStatusResult, String> {
    let initial_status = chatos_memory_engine::get_chatos_review_repair_status(req).await?;
    if !launch_result.accepted && !launch_result.running {
        return Ok(initial_status);
    }
    if let Some(final_status) =
        try_finalize_review_repair_status(req, launch_result, initial_status).await?
    {
        return Ok(final_status);
    }

    for _ in 0..REVIEW_REPAIR_POLL_MAX_ATTEMPTS {
        sleep(Duration::from_millis(REVIEW_REPAIR_POLL_INTERVAL_MS)).await;
        let status = chatos_memory_engine::get_chatos_review_repair_status(req).await?;
        if let Some(final_status) =
            try_finalize_review_repair_status(req, launch_result, status).await?
        {
            return Ok(final_status);
        }
    }

    Err(format!(
        "复盘任务等待超时，job_run_id={}",
        launch_result.job_run_id.as_deref().unwrap_or("unknown")
    ))
}

async fn try_finalize_review_repair_status(
    req: &chatos_memory_engine::ChatosReviewRepairRequest,
    launch_result: &chatos_memory_engine::ReviewRepairSummaryRunResult,
    status: chatos_memory_engine::ReviewRepairStatusResult,
) -> Result<Option<chatos_memory_engine::ReviewRepairStatusResult>, String> {
    if status.running {
        return Ok(None);
    }

    let Some(job_run_id) = launch_result.job_run_id.as_deref() else {
        return Ok(Some(status));
    };

    let Some(job_run) =
        chatos_memory_engine::get_chatos_review_repair_job_run(req, job_run_id).await?
    else {
        return Ok(Some(status));
    };

    match job_run.status.as_str() {
        "done" => Ok(Some(status)),
        "failed" => Err(job_run
            .error_message
            .unwrap_or_else(|| format!("复盘任务执行失败，job_run_id={job_run_id}"))),
        "running" => Ok(None),
        other => {
            warn!(
                "review repair job {} reached unexpected status {}",
                job_run_id, other
            );
            Ok(Some(status))
        }
    }
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
                &chatos_memory_engine::ChatosReviewRepairRequest { session },
            )
            .await
            {
                Ok(status) => status,
                Err(_) => final_status_candidate.unwrap_or_else(|| {
                    build_review_repair_completed_status_fallback(
                        req,
                        result,
                        initial_pending_count,
                    )
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
        &review_repair_status_dto(&final_status),
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

fn review_repair_status_dto(
    result: &chatos_memory_engine::ReviewRepairStatusResult,
) -> ReviewRepairStatusDto {
    ReviewRepairStatusDto {
        running: result.running,
        running_job_count: result.running_job_count,
        pending_message_count: result.pending_message_count,
        scope_session_count: result.scope_session_count,
        project_id: result.project_id.clone(),
        contact_id: result.contact_id.clone(),
        agent_id: result.agent_id.clone(),
        job_type: result.job_type.clone(),
    }
}
