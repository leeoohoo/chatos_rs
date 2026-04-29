use std::sync::Arc;

use tokio::sync::Semaphore;
use tokio::task::JoinSet;
use tracing::{info, warn};

use crate::ai::AiClient;
use crate::db::Db;
use crate::models::REVIEW_REPAIR_SUMMARY_PROMPT_TEMPLATE;
use crate::repositories::{configs, jobs as job_runs, messages, sessions};

use super::summary_generation::{process_session, process_session_force};
use super::{job_support, summary_support};

#[derive(Debug, Clone, serde::Serialize)]
pub struct SummaryRunResult {
    pub processed_sessions: usize,
    pub summarized_sessions: usize,
    pub generated_summaries: usize,
    pub marked_messages: usize,
    pub failed_sessions: usize,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ScopedSummaryRunResult {
    pub processed_sessions: usize,
    pub summarized_sessions: usize,
    pub generated_summaries: usize,
    pub marked_messages: usize,
    pub failed_sessions: usize,
    pub pending_message_count: i64,
    pub project_id: String,
    pub contact_id: Option<String>,
    pub agent_id: Option<String>,
    pub mode: String,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ScopedReviewRepairStatus {
    pub running: bool,
    pub running_job_count: i64,
    pub pending_message_count: i64,
    pub scope_session_count: usize,
    pub project_id: String,
    pub contact_id: Option<String>,
    pub agent_id: Option<String>,
    pub job_type: String,
}

pub async fn run_once(pool: &Db, ai: &AiClient, user_id: &str) -> Result<SummaryRunResult, String> {
    let config = configs::get_effective_summary_job_config(pool, user_id).await?;
    if config.enabled != 1 {
        return Ok(SummaryRunResult {
            processed_sessions: 0,
            summarized_sessions: 0,
            generated_summaries: 0,
            marked_messages: 0,
            failed_sessions: 0,
        });
    }

    let model_cfg = summary_support::resolve_model_config(
        pool,
        user_id,
        config.summary_model_config_id.as_deref(),
    )
    .await?;
    let model_name = model_cfg
        .as_ref()
        .map(|m| m.model.clone())
        .unwrap_or_else(|| "local-fallback".to_string());

    let session_ids = messages::list_session_ids_with_pending_messages_by_user(
        pool,
        user_id,
        config.max_sessions_per_tick,
    )
    .await?;

    let mut out = SummaryRunResult {
        processed_sessions: 0,
        summarized_sessions: 0,
        generated_summaries: 0,
        marked_messages: 0,
        failed_sessions: 0,
    };

    if session_ids.is_empty() {
        return Ok(out);
    }

    let concurrency = job_support::resolve_tick_concurrency(
        config.max_sessions_per_tick,
        "MEMORY_SERVER_SUMMARY_SESSION_CONCURRENCY",
        3,
    );
    info!(
        "[MEMORY-SUMMARY-L0] run_once user_id={} sessions={} concurrency={}",
        user_id,
        session_ids.len(),
        concurrency
    );

    out.processed_sessions = session_ids.len();
    let semaphore = Arc::new(Semaphore::new(concurrency));
    let mut join_set = JoinSet::new();

    for session_id in session_ids {
        let permit = semaphore
            .clone()
            .acquire_owned()
            .await
            .map_err(|err| err.to_string())?;
        let pool = pool.clone();
        let ai = ai.clone();
        let model_name = model_name.clone();
        let model_cfg = model_cfg.clone();
        let summary_prompt = config.summary_prompt.clone();
        let round_limit = config.round_limit;
        let token_limit = config.token_limit;
        let target_summary_tokens = config.target_summary_tokens;

        join_set.spawn(async move {
            let _permit = permit;
            let result = process_session(
                &pool,
                &ai,
                session_id.as_str(),
                &model_name,
                model_cfg.as_ref(),
                summary_prompt.as_deref(),
                round_limit,
                token_limit,
                target_summary_tokens,
            )
            .await;
            (session_id, result)
        });
    }

    while let Some(joined) = join_set.join_next().await {
        match joined {
            Ok((_session_id, Ok((generated, marked)))) => {
                if generated > 0 {
                    out.summarized_sessions += 1;
                }
                out.generated_summaries += generated;
                out.marked_messages += marked;
            }
            Ok((session_id, Err(err))) => {
                out.failed_sessions += 1;
                warn!(
                    "[MEMORY-SUMMARY-L0] process failed: user_id={} session_id={} error={}",
                    user_id, session_id, err
                );
            }
            Err(err) => {
                out.failed_sessions += 1;
                warn!(
                    "[MEMORY-SUMMARY-L0] process join failed: user_id={} error={}",
                    user_id, err
                );
            }
        }
    }

    Ok(out)
}

pub async fn run_once_for_session(
    pool: &Db,
    ai: &AiClient,
    user_id: &str,
    session_id: &str,
) -> Result<SummaryRunResult, String> {
    let config = configs::get_effective_summary_job_config(pool, user_id).await?;
    if config.enabled != 1 {
        return Ok(SummaryRunResult {
            processed_sessions: 0,
            summarized_sessions: 0,
            generated_summaries: 0,
            marked_messages: 0,
            failed_sessions: 0,
        });
    }

    let model_cfg = summary_support::resolve_model_config(
        pool,
        user_id,
        config.summary_model_config_id.as_deref(),
    )
    .await?;
    let model_name = model_cfg
        .as_ref()
        .map(|m| m.model.clone())
        .unwrap_or_else(|| "local-fallback".to_string());

    let (generated, marked) = process_session(
        pool,
        ai,
        session_id,
        &model_name,
        model_cfg.as_ref(),
        config.summary_prompt.as_deref(),
        config.round_limit,
        config.token_limit,
        config.target_summary_tokens,
    )
    .await?;

    Ok(SummaryRunResult {
        processed_sessions: 1,
        summarized_sessions: if generated > 0 { 1 } else { 0 },
        generated_summaries: generated,
        marked_messages: marked,
        failed_sessions: 0,
    })
}

pub async fn run_review_repair_for_scope(
    pool: &Db,
    ai: &AiClient,
    user_id: &str,
    project_id: &str,
    contact_id: Option<&str>,
    agent_id: Option<&str>,
) -> Result<ScopedSummaryRunResult, String> {
    let config = configs::get_effective_summary_job_config(pool, user_id).await?;

    let model_cfg = summary_support::resolve_model_config(
        pool,
        user_id,
        config.summary_model_config_id.as_deref(),
    )
    .await?;
    let model_name = model_cfg
        .as_ref()
        .map(|m| m.model.clone())
        .unwrap_or_else(|| "local-fallback".to_string());

    let session_ids = messages::list_session_ids_with_pending_messages_by_scope(
        pool,
        user_id,
        project_id,
        contact_id,
        agent_id,
        config.max_sessions_per_tick,
    )
    .await?;
    let pending_message_count = messages::count_pending_messages_by_scope(
        pool,
        user_id,
        project_id,
        contact_id,
        agent_id,
    )
    .await?;

    let mut out = ScopedSummaryRunResult {
        processed_sessions: session_ids.len(),
        summarized_sessions: 0,
        generated_summaries: 0,
        marked_messages: 0,
        failed_sessions: 0,
        pending_message_count,
        project_id: project_id.to_string(),
        contact_id: contact_id.map(ToOwned::to_owned),
        agent_id: agent_id.map(ToOwned::to_owned),
        mode: "review_repair".to_string(),
    };

    if session_ids.is_empty() {
        return Ok(out);
    }

    let concurrency = job_support::resolve_tick_concurrency(
        config.max_sessions_per_tick,
        "MEMORY_SERVER_SUMMARY_SESSION_CONCURRENCY",
        3,
    );

    info!(
        "[MEMORY-SUMMARY-REVIEW-REPAIR] run_once user_id={} project_id={} contact_id={:?} agent_id={:?} sessions={} concurrency={}",
        user_id,
        project_id,
        contact_id,
        agent_id,
        out.processed_sessions,
        concurrency
    );

    let semaphore = Arc::new(Semaphore::new(concurrency));
    let mut join_set = JoinSet::new();

    for session_id in session_ids {
        let permit = semaphore
            .clone()
            .acquire_owned()
            .await
            .map_err(|err| err.to_string())?;
        let pool = pool.clone();
        let ai = ai.clone();
        let model_name = model_name.clone();
        let model_cfg = model_cfg.clone();
        let round_limit = config.round_limit;
        let token_limit = config.token_limit;
        let target_summary_tokens = config.target_summary_tokens;

        join_set.spawn(async move {
            let _permit = permit;
            let result = process_session_force(
                &pool,
                &ai,
                session_id.as_str(),
                &model_name,
                model_cfg.as_ref(),
                Some(REVIEW_REPAIR_SUMMARY_PROMPT_TEMPLATE),
                round_limit,
                token_limit,
                target_summary_tokens,
                "summary_review_repair",
            )
            .await;
            (session_id, result)
        });
    }

    while let Some(joined) = join_set.join_next().await {
        match joined {
            Ok((_session_id, Ok((generated, marked)))) => {
                if generated > 0 {
                    out.summarized_sessions += 1;
                }
                out.generated_summaries += generated;
                out.marked_messages += marked;
            }
            Ok((session_id, Err(err))) => {
                out.failed_sessions += 1;
                warn!(
                    "[MEMORY-SUMMARY-REVIEW-REPAIR] process failed: user_id={} session_id={} error={}",
                    user_id, session_id, err
                );
            }
            Err(err) => {
                out.failed_sessions += 1;
                warn!(
                    "[MEMORY-SUMMARY-REVIEW-REPAIR] process join failed: user_id={} error={}",
                    user_id, err
                );
            }
        }
    }

    Ok(out)
}

pub async fn get_review_repair_status_for_scope(
    pool: &Db,
    user_id: &str,
    project_id: &str,
    contact_id: Option<&str>,
    agent_id: Option<&str>,
) -> Result<ScopedReviewRepairStatus, String> {
    let session_ids = sessions::list_session_ids_by_scope(
        pool,
        user_id,
        project_id,
        contact_id,
        agent_id,
        5000,
    )
    .await?;
    let pending_message_count = messages::count_pending_messages_by_scope(
        pool,
        user_id,
        project_id,
        contact_id,
        agent_id,
    )
    .await?;
    let running_job_count = job_runs::count_job_runs_for_sessions(
        pool,
        "summary_review_repair",
        session_ids.as_slice(),
        Some("running"),
    )
    .await?;

    Ok(ScopedReviewRepairStatus {
        running: running_job_count > 0,
        running_job_count,
        pending_message_count,
        scope_session_count: session_ids.len(),
        project_id: project_id.to_string(),
        contact_id: contact_id.map(ToOwned::to_owned),
        agent_id: agent_id.map(ToOwned::to_owned),
        job_type: "summary_review_repair".to_string(),
    })
}
