use std::sync::Arc;

use tokio::sync::Semaphore;
use tokio::task::JoinSet;
use tracing::{info, warn};

use crate::ai::AiClient;
use crate::db::Db;
use crate::models::{AiModelConfig, CreateSummaryInput, SessionSummary};
use crate::repositories::{auth::ADMIN_USER_ID, configs, jobs, summaries};
use crate::services::summarizer::{
    estimate_tokens_text, summarize_texts_with_split, summary_to_rollup_block,
};

#[derive(Debug, Clone, serde::Serialize)]
pub struct RollupRunResult {
    pub processed_sessions: usize,
    pub rolled_up_sessions: usize,
    pub generated_summaries: usize,
    pub marked_summaries: usize,
    pub failed_sessions: usize,
}

#[derive(Debug, Clone)]
struct RollupBatchSelection {
    level: i64,
    selected: Vec<SessionSummary>,
    trigger_reason: &'static str,
}

pub async fn run_once(pool: &Db, ai: &AiClient, user_id: &str) -> Result<RollupRunResult, String> {
    let config = configs::get_summary_rollup_job_config(pool, user_id).await?;
    if config.enabled != 1 {
        return Ok(RollupRunResult {
            processed_sessions: 0,
            rolled_up_sessions: 0,
            generated_summaries: 0,
            marked_summaries: 0,
            failed_sessions: 0,
        });
    }

    let model_cfg = resolve_model_config(pool, user_id, config.summary_model_config_id.as_deref()).await?;
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

    let concurrency = resolve_session_concurrency(
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

fn resolve_session_concurrency(max_sessions_per_tick: i64, env_key: &str, default_limit: usize) -> usize {
    let configured = std::env::var(env_key)
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .filter(|v| *v > 0)
        .unwrap_or(default_limit.max(1));
    let cap = max_sessions_per_tick.max(1) as usize;
    configured.min(cap).max(1)
}

async fn process_session(
    pool: &Db,
    ai: &AiClient,
    session_id: &str,
    model_name: &str,
    model_cfg: Option<&AiModelConfig>,
    round_limit: i64,
    token_limit: i64,
    target_summary_tokens: i64,
    keep_raw_level0_count: i64,
    max_level: i64,
) -> Result<(usize, usize), String> {
    let selection = select_rollup_batch(
        pool,
        session_id,
        round_limit.max(1),
        token_limit.max(500),
        keep_raw_level0_count.max(0),
        max_level.max(1),
    )
    .await?;

    let Some(selection) = selection else {
        return Ok((0, 0));
    };
    let level = selection.level;
    let selected = selection.selected;
    let trigger_reason = selection.trigger_reason;

    let target_level = level + 1;

    let mut summarizable = Vec::new();
    let mut oversized = Vec::new();
    for summary in &selected {
        let block = summary_to_rollup_block(summary);
        let tokens = estimate_tokens_text(block.as_str());
        if tokens > token_limit.max(500) {
            oversized.push(summary.clone());
        } else {
            summarizable.push(block);
        }
    }

    let selected_ids: Vec<String> = selected.iter().map(|s| s.id.clone()).collect();
    let selected_tokens = selected
        .iter()
        .map(|s| estimate_tokens_text(s.summary_text.as_str()))
        .sum::<i64>();

    let trigger = format!("rollup_level_{}_to_{}", level, target_level);
    let trigger_with_reason = format!("{}+{}", trigger, trigger_reason);
    let job_run = jobs::create_job_run(
        pool,
        "summary_rollup",
        Some(session_id),
        Some(trigger_with_reason.as_str()),
        selected.len() as i64,
    )
    .await?;

    let mut overflow_retry_count = 0usize;
    let mut forced_truncated = false;
    let summary_text: String = if summarizable.is_empty() {
        format!(
            "本批次 level={} 的 {} 条总结全部超出 token_limit={}，已仅做层级标记处理。",
            level,
            oversized.len(),
            token_limit
        )
    } else {
        let build = match summarize_texts_with_split(
            ai,
            model_cfg,
            &format!("会话总结再总结 level {} -> {}", level, target_level),
            summarizable.as_slice(),
            token_limit,
            target_summary_tokens,
        )
        .await {
            Ok(v) => v,
            Err(err) => {
                let _ = finish_failed_job_run(pool, job_run.id.as_str(), err.as_str()).await;
                return Err(err);
            }
        };
        let mut merged = build.text;
        if build.chunk_count > 1 {
            merged.push_str(&format!(
                "\n\n[meta] 该 rollup 由 {} 个分片合并。",
                build.chunk_count
            ));
        }
        if build.overflow_retry_count > 0 {
            merged.push_str(&format!(
                "\n\n[meta] 发生上下文溢出并自动重试 {} 次后成功。",
                build.overflow_retry_count
            ));
        }
        if build.forced_truncated {
            merged.push_str(
                "\n\n[meta] 本次 rollup 触发强制截断兜底，已标记该批次总结为已 rollup。",
            );
        }
        if !oversized.is_empty() {
            merged.push_str(&format!(
                "\n\n[meta] {} 条超长总结未纳入正文，但已标记为已 rollup。",
                oversized.len()
            ));
        }
        overflow_retry_count = build.overflow_retry_count;
        forced_truncated = build.forced_truncated;
        merged
    };

    let mut trigger_type = trigger_with_reason;
    if !oversized.is_empty() {
        trigger_type.push_str("+oversized_single_skipped");
    }
    if overflow_retry_count > 0 {
        trigger_type.push_str("+overflow_retry");
    }
    if forced_truncated {
        trigger_type.push_str("+forced_truncated");
    }

    let summary = match summaries::create_summary(
        pool,
        CreateSummaryInput {
            session_id: session_id.to_string(),
            summary_text,
            summary_model: model_name.to_string(),
            trigger_type,
            source_start_message_id: selected.first().map(|s| s.id.clone()),
            source_end_message_id: selected.last().map(|s| s.id.clone()),
            source_message_count: selected.len() as i64,
            source_estimated_tokens: selected_tokens,
            status: "done".to_string(),
            error_message: None,
            level: target_level,
        },
    )
    .await {
        Ok(v) => v,
        Err(err) => {
            let _ = finish_failed_job_run(pool, job_run.id.as_str(), err.as_str()).await;
            return Err(err);
        }
    };

    let marked = match summaries::mark_summaries_rolled_up(
        pool,
        selected_ids.as_slice(),
        summary.id.as_str(),
    )
    .await
    {
        Ok(v) => v,
        Err(err) => {
            let _ = finish_failed_job_run(pool, job_run.id.as_str(), err.as_str()).await;
            return Err(err);
        }
    };
    if let Err(err) = jobs::finish_job_run(pool, job_run.id.as_str(), "done", 1, None).await {
        warn!(
            "[MEMORY-SUMMARY-ROLLUP] finish job run failed: session_id={} job_run_id={} error={}",
            session_id, job_run.id, err
        );
    }

    info!(
        "[MEMORY-SUMMARY-ROLLUP] done session_id={} level={}->{} selected={} marked={} summary_id={}",
        session_id,
        level,
        target_level,
        selected.len(),
        marked,
        summary.id
    );

    Ok((1, marked))
}

async fn finish_failed_job_run(pool: &Db, job_run_id: &str, error_message: &str) {
    if let Err(err) = jobs::finish_job_run(pool, job_run_id, "failed", 0, Some(error_message)).await
    {
        warn!(
            "[MEMORY-SUMMARY-ROLLUP] mark job failed status failed: job_run_id={} error={}",
            job_run_id, err
        );
    }
}

async fn select_rollup_batch(
    pool: &Db,
    session_id: &str,
    round_limit: i64,
    token_limit: i64,
    keep_raw_level0_count: i64,
    max_level: i64,
) -> Result<Option<RollupBatchSelection>, String> {
    for level in 0..max_level {
        let mut candidates = summaries::list_done_pending_rollup_summaries_by_level_no_limit(pool, session_id, level).await?;
        if level == 0 && keep_raw_level0_count > 0 {
            let keep = keep_raw_level0_count as usize;
            if candidates.len() > keep {
                candidates = candidates.into_iter().skip(keep).collect();
            } else {
                candidates.clear();
            }
        }

        if candidates.is_empty() {
            continue;
        }

        if candidates.len() as i64 >= round_limit {
            let selected: Vec<SessionSummary> = candidates
                .iter()
                .take(round_limit as usize)
                .cloned()
                .collect();
            return Ok(Some(RollupBatchSelection {
                level,
                selected,
                trigger_reason: "message_count_limit",
            }));
        }

        let token_sum = candidates
            .iter()
            .map(summary_to_rollup_block)
            .map(|text| estimate_tokens_text(text.as_str()))
            .sum::<i64>();
        if token_sum >= token_limit {
            return Ok(Some(RollupBatchSelection {
                level,
                selected: candidates,
                trigger_reason: "token_limit",
            }));
        }
    }

    Ok(None)
}

async fn resolve_model_config(
    pool: &Db,
    user_id: &str,
    model_config_id: Option<&str>,
) -> Result<Option<AiModelConfig>, String> {
    if let Some(id) = model_config_id {
        if let Some(cfg) = configs::get_model_config_by_id(pool, id).await? {
            if (cfg.user_id == user_id || cfg.user_id == ADMIN_USER_ID) && cfg.enabled == 1 {
                return Ok(Some(cfg));
            }
        }
    }

    let all = configs::list_model_configs(pool, user_id).await?;
    if let Some(cfg) = all.into_iter().find(|c| c.enabled == 1) {
        return Ok(Some(cfg));
    }

    if user_id != ADMIN_USER_ID {
        let admin_all = configs::list_model_configs(pool, ADMIN_USER_ID).await?;
        return Ok(admin_all.into_iter().find(|c| c.enabled == 1));
    }

    Ok(None)
}
