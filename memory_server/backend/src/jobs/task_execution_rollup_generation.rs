use tracing::{info, warn};

use crate::ai::AiClient;
use crate::db::Db;
use crate::models::{
    AiModelConfig, CreateTaskExecutionSummaryInput, TaskExecutionScope, TaskExecutionSummary,
};
use crate::repositories::{jobs, locks, task_execution_summaries};
use crate::services::summarizer::{estimate_tokens_text, summarize_texts_with_split};

use super::{idempotency, job_support};

#[derive(Debug, Clone)]
struct RollupBatchSelection {
    level: i64,
    selected: Vec<TaskExecutionSummary>,
    trigger_reason: &'static str,
}

pub(crate) async fn process_scope(
    pool: &Db,
    ai: &AiClient,
    scope: &TaskExecutionScope,
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
    let lock_key = format!("task_execution_rollup:{}", scope.scope_key);
    let Some(lock_handle) =
        locks::try_acquire_job_lock(pool, lock_key.as_str(), lease_seconds).await?
    else {
        info!(
            "[MEMORY-TASK-EXEC-ROLLUP] skip scope lock busy: scope_key={}",
            scope.scope_key
        );
        return Ok((0, 0));
    };

    let result = process_scope_locked(
        pool,
        ai,
        scope,
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
            "[MEMORY-TASK-EXEC-ROLLUP] release lock failed: scope_key={} key={} error={}",
            scope.scope_key, lock_handle.lock_key, err
        );
    }

    result
}

#[allow(clippy::too_many_arguments)]
async fn process_scope_locked(
    pool: &Db,
    ai: &AiClient,
    scope: &TaskExecutionScope,
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
        scope,
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
        let block = task_summary_to_rollup_block(summary);
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
    let digest_namespace = format!("task_execution_rollup:l{}->{}", level, target_level);
    let source_digest =
        idempotency::digest_from_ids(digest_namespace.as_str(), selected_ids.as_slice())
            .ok_or_else(|| "build task execution rollup source digest failed".to_string())?;

    if let Some(existing) = task_execution_summaries::find_summary_by_source_digest(
        pool,
        scope.user_id.as_str(),
        scope.contact_agent_id.as_str(),
        scope.project_id.as_str(),
        target_level,
        source_digest.as_str(),
    )
    .await?
    {
        if let Err(err) = locks::refresh_job_lock(pool, lock_handle, lease_seconds).await {
            warn!(
                "[MEMORY-TASK-EXEC-ROLLUP] refresh lock failed before reuse: scope_key={} error={}",
                scope.scope_key, err
            );
        }
        let marked = task_execution_summaries::mark_summaries_rolled_up(
            pool,
            selected_ids.as_slice(),
            existing.id.as_str(),
        )
        .await?;
        if marked < selected_ids.len() {
            warn!(
                "[MEMORY-TASK-EXEC-ROLLUP] partial mark on reuse: scope_key={} selected={} marked={} summary_id={}",
                scope.scope_key,
                selected_ids.len(),
                marked,
                existing.id
            );
        }
        info!(
            "[MEMORY-TASK-EXEC-ROLLUP] reused existing summary by digest: scope_key={} level={}->{} digest={} summary_id={} marked={}",
            scope.scope_key, level, target_level, source_digest, existing.id, marked
        );
        return Ok((0, marked));
    }

    let trigger = format!(
        "task_execution_rollup_level_{}_to_{}+{}",
        level, target_level, trigger_reason
    );
    let job_run = match jobs::create_job_run(
        pool,
        "task_execution_rollup",
        Some(scope.scope_key.as_str()),
        Some(trigger.as_str()),
        selected.len() as i64,
    )
    .await
    {
        Ok(v) => v,
        Err(err) if jobs::is_already_running_error(err.as_str()) => {
            info!(
                "[MEMORY-TASK-EXEC-ROLLUP] skip scope already running: scope_key={}",
                scope.scope_key
            );
            return Ok((0, 0));
        }
        Err(err) => return Err(err),
    };

    if let Err(err) = locks::refresh_job_lock(pool, lock_handle, lease_seconds).await {
        warn!(
            "[MEMORY-TASK-EXEC-ROLLUP] refresh lock failed before llm: scope_key={} error={}",
            scope.scope_key, err
        );
    }

    let mut overflow_retry_count = 0usize;
    let mut forced_truncated = false;
    let summary_text: String = if summarizable.is_empty() {
        format!(
            "本批次 level={} 的 {} 条任务执行总结全部超出 token_limit={}，已仅做层级标记处理。",
            level,
            oversized.len(),
            token_limit
        )
    } else {
        let build = match summarize_texts_with_split(
            ai,
            model_cfg,
            &format!("任务执行总结再总结 level {} -> {}", level, target_level),
            summary_prompt,
            summarizable.as_slice(),
            token_limit,
            target_summary_tokens,
        )
        .await
        {
            Ok(v) => v,
            Err(err) => {
                finish_failed_job_run(pool, job_run.id.as_str(), err.as_str()).await;
                return Err(err);
            }
        };
        let mut merged = build.text;
        if build.chunk_count > 1 {
            merged.push_str(&format!(
                "\n\n[meta] 该任务执行 rollup 由 {} 个分片合并。",
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
                "\n\n[meta] 本次任务执行 rollup 触发强制截断兜底，已标记该批次总结为已 rollup。",
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

    let mut trigger_type = trigger;
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
            "[MEMORY-TASK-EXEC-ROLLUP] refresh lock failed before persist: scope_key={} error={}",
            scope.scope_key, err
        );
    }

    let create_result = match task_execution_summaries::create_summary(
        pool,
        CreateTaskExecutionSummaryInput {
            user_id: scope.user_id.clone(),
            contact_agent_id: scope.contact_agent_id.clone(),
            project_id: scope.project_id.clone(),
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
            finish_failed_job_run(pool, job_run.id.as_str(), err.as_str()).await;
            return Err(err);
        }
    };
    let generated = if create_result.inserted { 1 } else { 0 };
    let summary = create_result.summary;

    if let Err(err) = locks::refresh_job_lock(pool, lock_handle, lease_seconds).await {
        warn!(
            "[MEMORY-TASK-EXEC-ROLLUP] refresh lock failed before mark: scope_key={} error={}",
            scope.scope_key, err
        );
    }

    let marked = match task_execution_summaries::mark_summaries_rolled_up(
        pool,
        selected_ids.as_slice(),
        summary.id.as_str(),
    )
    .await
    {
        Ok(v) => v,
        Err(err) => {
            finish_failed_job_run(pool, job_run.id.as_str(), err.as_str()).await;
            return Err(err);
        }
    };
    if marked < selected_ids.len() {
        warn!(
            "[MEMORY-TASK-EXEC-ROLLUP] partial mark: scope_key={} selected={} marked={} summary_id={}",
            scope.scope_key,
            selected_ids.len(),
            marked,
            summary.id
        );
    }

    if let Err(err) =
        jobs::finish_job_run(pool, job_run.id.as_str(), "done", generated as i64, None).await
    {
        warn!(
            "[MEMORY-TASK-EXEC-ROLLUP] finish job run failed: scope_key={} job_run_id={} error={}",
            scope.scope_key, job_run.id, err
        );
    }

    info!(
        "[MEMORY-TASK-EXEC-ROLLUP] done scope_key={} level={}->{} selected={} marked={} generated={} summary_id={}",
        scope.scope_key,
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
    job_support::finish_failed_job_run(
        pool,
        job_run_id,
        error_message,
        "[MEMORY-TASK-EXEC-ROLLUP]",
    )
    .await;
}

async fn select_rollup_batch(
    pool: &Db,
    scope: &TaskExecutionScope,
    round_limit: i64,
    token_limit: i64,
    keep_raw_level0_count: i64,
    max_level: i64,
) -> Result<Option<RollupBatchSelection>, String> {
    for level in 0..max_level {
        let mut candidates = task_execution_summaries::list_pending_summaries_by_level_no_limit(
            pool,
            scope.user_id.as_str(),
            scope.contact_agent_id.as_str(),
            scope.project_id.as_str(),
            level,
        )
        .await?;
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
            let selected = candidates
                .iter()
                .take(round_limit as usize)
                .cloned()
                .collect();
            return Ok(Some(RollupBatchSelection {
                level,
                selected,
                trigger_reason: "summary_count_limit",
            }));
        }

        let token_sum = candidates
            .iter()
            .map(task_summary_to_rollup_block)
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

fn task_summary_to_rollup_block(summary: &TaskExecutionSummary) -> String {
    format!(
        "[level={}][created_at={}][id={}]\n{}",
        summary.level, summary.created_at, summary.id, summary.summary_text
    )
}
