use std::sync::Arc;

use tokio::sync::Semaphore;
use tokio::task::JoinSet;
use tracing::{info, warn};

use crate::ai::AiClient;
use crate::db::Db;
use crate::repositories::{configs, task_execution_summaries};

use super::job_support;
use super::task_execution_rollup_generation::process_scope;

#[derive(Debug, Clone, serde::Serialize)]
pub struct TaskExecutionRollupRunResult {
    pub processed_scopes: usize,
    pub rolled_up_scopes: usize,
    pub generated_summaries: usize,
    pub marked_summaries: usize,
    pub failed_scopes: usize,
}

pub async fn run_once(
    pool: &Db,
    ai: &AiClient,
    user_id: &str,
) -> Result<TaskExecutionRollupRunResult, String> {
    let config = configs::get_effective_task_execution_rollup_job_config(pool, user_id).await?;
    if config.enabled != 1 {
        return Ok(TaskExecutionRollupRunResult {
            processed_scopes: 0,
            rolled_up_scopes: 0,
            generated_summaries: 0,
            marked_summaries: 0,
            failed_scopes: 0,
        });
    }

    let model_cfg =
        job_support::resolve_model_config(pool, user_id, config.summary_model_config_id.as_deref())
            .await?;
    let model_name = model_cfg
        .as_ref()
        .map(|m| m.model.clone())
        .unwrap_or_else(|| "local-fallback".to_string());

    let scopes = task_execution_summaries::list_scope_keys_with_pending_rollup_by_user(
        pool,
        user_id,
        config.max_level,
        config.max_scopes_per_tick,
    )
    .await?;

    let mut out = TaskExecutionRollupRunResult {
        processed_scopes: 0,
        rolled_up_scopes: 0,
        generated_summaries: 0,
        marked_summaries: 0,
        failed_scopes: 0,
    };

    if scopes.is_empty() {
        return Ok(out);
    }

    let concurrency = job_support::resolve_tick_concurrency(
        config.max_scopes_per_tick,
        "MEMORY_SERVER_TASK_EXECUTION_ROLLUP_CONCURRENCY",
        3,
    );
    info!(
        "[MEMORY-TASK-EXEC-ROLLUP] run_once user_id={} scopes={} concurrency={}",
        user_id,
        scopes.len(),
        concurrency
    );

    out.processed_scopes = scopes.len();
    let semaphore = Arc::new(Semaphore::new(concurrency));
    let mut join_set = JoinSet::new();

    for scope in scopes {
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
        let keep_raw_level0_count = config.keep_raw_level0_count;
        let max_level = config.max_level;

        join_set.spawn(async move {
            let _permit = permit;
            let result = process_scope(
                &pool,
                &ai,
                &scope,
                &model_name,
                model_cfg.as_ref(),
                summary_prompt.as_deref(),
                round_limit,
                token_limit,
                target_summary_tokens,
                keep_raw_level0_count,
                max_level,
            )
            .await;
            (scope, result)
        });
    }

    while let Some(joined) = join_set.join_next().await {
        match joined {
            Ok((_scope, Ok((generated, marked)))) => {
                if generated > 0 {
                    out.rolled_up_scopes += 1;
                }
                out.generated_summaries += generated;
                out.marked_summaries += marked;
            }
            Ok((scope, Err(err))) => {
                out.failed_scopes += 1;
                warn!(
                    "[MEMORY-TASK-EXEC-ROLLUP] process failed: user_id={} scope_key={} error={}",
                    user_id, scope.scope_key, err
                );
            }
            Err(err) => {
                out.failed_scopes += 1;
                warn!(
                    "[MEMORY-TASK-EXEC-ROLLUP] process join failed: user_id={} error={}",
                    user_id, err
                );
            }
        }
    }

    Ok(out)
}
