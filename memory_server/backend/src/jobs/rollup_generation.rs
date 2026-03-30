use tracing::{info, warn};

use crate::ai::AiClient;
use crate::db::Db;
use crate::models::{AiModelConfig, CreateSummaryInput, SessionSummary};
use crate::repositories::{jobs, locks, summaries};
use crate::services::summarizer::{
    estimate_tokens_text, summarize_texts_with_split, summary_to_rollup_block,
};

use super::{idempotency, job_support, memory_sync};

#[derive(Debug, Clone)]
struct RollupBatchSelection {
    level: i64,
    selected: Vec<SessionSummary>,
    trigger_reason: &'static str,
}

pub(crate) async fn process_session(
    pool: &Db,
    ai: &AiClient,
    session_id: &str,
    model_name: &str,
    model_cfg: Option<&AiModelConfig>,
    summary_prompt: Option<&str>,
    round_limit: i64,
    token_limit: i64,
    target_summary_tokens: i64,
    keep_raw_level0_count: i64,
    max_level: i64,
) -> Result<(usize, usize), String> {
    let lease_seconds = job_support::resolve_lock_lease_seconds();
    let lock_key = format!("summary_rollup:{}", session_id);
    let Some(lock_handle) =
        locks::try_acquire_job_lock(pool, lock_key.as_str(), lease_seconds).await?
    else {
        info!(
            "[MEMORY-SUMMARY-ROLLUP] skip session lock busy: session_id={}",
            session_id
        );
        return Ok((0, 0));
    };

    let result = process_session_locked(
        pool,
        ai,
        session_id,
        model_name,
        model_cfg,
        summary_prompt,
        round_limit,
        token_limit,
        target_summary_tokens,
        keep_raw_level0_count,
        max_level,
        &lock_handle,
        lease_seconds,
    )
    .await;

    if let Err(err) = locks::release_job_lock(pool, &lock_handle).await {
        warn!(
            "[MEMORY-SUMMARY-ROLLUP] release lock failed: session_id={} key={} error={}",
            session_id, lock_handle.lock_key, err
        );
    }

    result
}

#[allow(clippy::too_many_arguments)]
async fn process_session_locked(
    pool: &Db,
    ai: &AiClient,
    session_id: &str,
    model_name: &str,
    model_cfg: Option<&AiModelConfig>,
    summary_prompt: Option<&str>,
    round_limit: i64,
    token_limit: i64,
    target_summary_tokens: i64,
    keep_raw_level0_count: i64,
    max_level: i64,
    lock_handle: &locks::JobLockHandle,
    lease_seconds: i64,
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
    let digest_namespace = format!("summary_rollup:l{}->{}", level, target_level);
    let source_digest =
        idempotency::digest_from_ids(digest_namespace.as_str(), selected_ids.as_slice())
            .ok_or_else(|| "build rollup source digest failed".to_string())?;

    if let Some(existing) = summaries::find_summary_by_source_digest(
        pool,
        session_id,
        target_level,
        source_digest.as_str(),
    )
    .await?
    {
        if let Err(err) = locks::refresh_job_lock(pool, lock_handle, lease_seconds).await {
            warn!(
                "[MEMORY-SUMMARY-ROLLUP] refresh lock failed before reuse: session_id={} error={}",
                session_id, err
            );
        }

        if let Err(err) = memory_sync::sync_memories_from_summary(pool, session_id, &existing).await
        {
            return Err(err);
        }

        let marked = summaries::mark_summaries_rolled_up(
            pool,
            selected_ids.as_slice(),
            existing.id.as_str(),
        )
        .await?;
        if marked < selected_ids.len() {
            warn!(
                "[MEMORY-SUMMARY-ROLLUP] partial mark on reuse: session_id={} selected={} marked={} summary_id={}",
                session_id,
                selected_ids.len(),
                marked,
                existing.id
            );
        }
        info!(
            "[MEMORY-SUMMARY-ROLLUP] reused existing summary by digest: session_id={} level={}->{} digest={} summary_id={} marked={}",
            session_id, level, target_level, source_digest, existing.id, marked
        );
        return Ok((0, marked));
    }

    let trigger = format!("rollup_level_{}_to_{}", level, target_level);
    let trigger_with_reason = format!("{}+{}", trigger, trigger_reason);
    let job_run = match jobs::create_job_run(
        pool,
        "summary_rollup",
        Some(session_id),
        Some(trigger_with_reason.as_str()),
        selected.len() as i64,
    )
    .await
    {
        Ok(v) => v,
        Err(err) if jobs::is_already_running_error(err.as_str()) => {
            info!(
                "[MEMORY-SUMMARY-ROLLUP] skip session already running: session_id={}",
                session_id
            );
            return Ok((0, 0));
        }
        Err(err) => return Err(err),
    };

    if let Err(err) = locks::refresh_job_lock(pool, lock_handle, lease_seconds).await {
        warn!(
            "[MEMORY-SUMMARY-ROLLUP] refresh lock failed before llm: session_id={} error={}",
            session_id, err
        );
    }

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
            summary_prompt,
            summarizable.as_slice(),
            token_limit,
            target_summary_tokens,
        )
        .await
        {
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
            merged
                .push_str("\n\n[meta] 本次 rollup 触发强制截断兜底，已标记该批次总结为已 rollup。");
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

    if let Err(err) = locks::refresh_job_lock(pool, lock_handle, lease_seconds).await {
        warn!(
            "[MEMORY-SUMMARY-ROLLUP] refresh lock failed before persist: session_id={} error={}",
            session_id, err
        );
    }

    let create_result = match summaries::create_summary(
        pool,
        CreateSummaryInput {
            session_id: session_id.to_string(),
            source_digest: Some(source_digest.clone()),
            summary_text,
            summary_model: model_name.to_string(),
            trigger_type,
            source_start_message_id: selected.first().map(|s| s.id.clone()),
            source_end_message_id: selected.last().map(|s| s.id.clone()),
            source_message_count: selected.len() as i64,
            source_estimated_tokens: selected_tokens,
            status: "pending".to_string(),
            error_message: None,
            level: target_level,
        },
    )
    .await
    {
        Ok(v) => v,
        Err(err) => {
            let _ = finish_failed_job_run(pool, job_run.id.as_str(), err.as_str()).await;
            return Err(err);
        }
    };
    let generated = if create_result.inserted { 1 } else { 0 };
    let summary = create_result.summary;

    if let Err(err) = memory_sync::sync_memories_from_summary(pool, session_id, &summary).await {
        let _ = finish_failed_job_run(pool, job_run.id.as_str(), err.as_str()).await;
        return Err(err);
    }

    if let Err(err) = locks::refresh_job_lock(pool, lock_handle, lease_seconds).await {
        warn!(
            "[MEMORY-SUMMARY-ROLLUP] refresh lock failed before mark: session_id={} error={}",
            session_id, err
        );
    }

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
    if marked < selected_ids.len() {
        warn!(
            "[MEMORY-SUMMARY-ROLLUP] partial mark: session_id={} selected={} marked={} summary_id={}",
            session_id,
            selected_ids.len(),
            marked,
            summary.id
        );
    }

    if let Err(err) =
        jobs::finish_job_run(pool, job_run.id.as_str(), "done", generated as i64, None).await
    {
        warn!(
            "[MEMORY-SUMMARY-ROLLUP] finish job run failed: session_id={} job_run_id={} error={}",
            session_id, job_run.id, err
        );
    }

    info!(
        "[MEMORY-SUMMARY-ROLLUP] done session_id={} level={}->{} selected={} marked={} generated={} summary_id={}",
        session_id,
        level,
        target_level,
        selected.len(),
        marked,
        generated,
        summary.id
    );

    Ok((generated, marked))
}

async fn finish_failed_job_run(pool: &Db, job_run_id: &str, error_message: &str) {
    job_support::finish_failed_job_run(pool, job_run_id, error_message, "[MEMORY-SUMMARY-ROLLUP]")
        .await;
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
        let mut candidates =
            summaries::list_pending_summaries_by_level_no_limit(pool, session_id, level).await?;
        if level == 0 && keep_raw_level0_count > 0 {
            let keep = keep_raw_level0_count as usize;
            if candidates.len() > keep {
                let rollup_len = candidates.len().saturating_sub(keep);
                candidates.truncate(rollup_len);
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
