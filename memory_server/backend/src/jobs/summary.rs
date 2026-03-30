use std::sync::Arc;

use tokio::sync::Semaphore;
use tokio::task::JoinSet;
use tracing::{info, warn};

use crate::ai::AiClient;
use crate::db::Db;
use crate::repositories::{configs, messages};

use super::summary_generation::process_session;
use super::{job_support, summary_support};

#[derive(Debug, Clone, serde::Serialize)]
pub struct SummaryRunResult {
    pub processed_sessions: usize,
    pub summarized_sessions: usize,
    pub generated_summaries: usize,
    pub marked_messages: usize,
    pub failed_sessions: usize,
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
