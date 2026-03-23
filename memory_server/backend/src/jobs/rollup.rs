use std::sync::Arc;

use tokio::sync::Semaphore;
use tokio::task::JoinSet;
use tracing::{info, warn};

use crate::ai::AiClient;
use crate::db::Db;
use crate::repositories::{configs, summaries};

use super::job_support;
use super::rollup_generation::process_session;

#[derive(Debug, Clone, serde::Serialize)]
pub struct RollupRunResult {
    pub processed_sessions: usize,
    pub rolled_up_sessions: usize,
    pub generated_summaries: usize,
    pub marked_summaries: usize,
    pub failed_sessions: usize,
}

pub async fn run_once(pool: &Db, ai: &AiClient, user_id: &str) -> Result<RollupRunResult, String> {
    let config = configs::get_effective_summary_rollup_job_config(pool, user_id).await?;
    if config.enabled != 1 {
        return Ok(RollupRunResult {
            processed_sessions: 0,
            rolled_up_sessions: 0,
            generated_summaries: 0,
            marked_summaries: 0,
            failed_sessions: 0,
        });
    }

    let model_cfg =
        job_support::resolve_model_config(pool, user_id, config.summary_model_config_id.as_deref())
            .await?;
    let model_name = model_cfg
        .as_ref()
        .map(|m| m.model.clone())
        .unwrap_or_else(|| "local-fallback".to_string());

    let session_ids = summaries::list_session_ids_with_pending_rollup_by_user(
        pool,
        user_id,
        config.max_level,
        config.max_sessions_per_tick,
    )
    .await?;

    let mut out = RollupRunResult {
        processed_sessions: 0,
        rolled_up_sessions: 0,
        generated_summaries: 0,
        marked_summaries: 0,
        failed_sessions: 0,
    };

    if session_ids.is_empty() {
        return Ok(out);
    }

    let concurrency = job_support::resolve_tick_concurrency(
        config.max_sessions_per_tick,
        "MEMORY_SERVER_ROLLUP_SESSION_CONCURRENCY",
        3,
    );
    info!(
        "[MEMORY-SUMMARY-ROLLUP] run_once user_id={} sessions={} concurrency={}",
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
        let round_limit = config.round_limit;
        let token_limit = config.token_limit;
        let target_summary_tokens = config.target_summary_tokens;
        let keep_raw_level0_count = config.keep_raw_level0_count;
        let max_level = config.max_level;

        join_set.spawn(async move {
            let _permit = permit;
            let result = process_session(
                &pool,
                &ai,
                session_id.as_str(),
                &model_name,
                model_cfg.as_ref(),
                round_limit,
                token_limit,
                target_summary_tokens,
                keep_raw_level0_count,
                max_level,
            )
            .await;
            (session_id, result)
        });
    }

    while let Some(joined) = join_set.join_next().await {
        match joined {
            Ok((_session_id, Ok((generated, marked)))) => {
                if generated > 0 {
                    out.rolled_up_sessions += 1;
                }
                out.generated_summaries += generated;
                out.marked_summaries += marked;
            }
            Ok((session_id, Err(err))) => {
                out.failed_sessions += 1;
                warn!(
                    "[MEMORY-SUMMARY-ROLLUP] process failed: session_id={} error={}",
                    session_id, err
                );
            }
            Err(err) => {
                out.failed_sessions += 1;
                warn!(
                    "[MEMORY-SUMMARY-ROLLUP] process join failed: user_id={} error={}",
                    user_id, err
                );
            }
        }
    }

    Ok(out)
}
