use tracing::{info, warn};

use crate::ai::AiClient;
use crate::db::Db;
use crate::models::{AiModelConfig, CreateSummaryInput};
use crate::repositories::{jobs, locks, messages, summaries};
use crate::services::summarizer::{
    estimate_tokens_text, estimate_tokens_texts, message_to_summary_block,
    summarize_texts_with_split,
};

use super::{idempotency, job_support, memory_sync, summary_support};

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
) -> Result<(usize, usize), String> {
    let lease_seconds = job_support::resolve_lock_lease_seconds();
    let lock_key = format!("summary_l0:{}", session_id);
    let Some(lock_handle) =
        locks::try_acquire_job_lock(pool, lock_key.as_str(), lease_seconds).await?
    else {
        info!(
            "[MEMORY-SUMMARY-L0] skip session lock busy: session_id={}",
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
        &lock_handle,
        lease_seconds,
    )
    .await;

    if let Err(err) = locks::release_job_lock(pool, &lock_handle).await {
        warn!(
            "[MEMORY-SUMMARY-L0] release lock failed: session_id={} key={} error={}",
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
    lock_handle: &locks::JobLockHandle,
    lease_seconds: i64,
) -> Result<(usize, usize), String> {
    let pending_head =
        messages::list_pending_messages(pool, session_id, Some(round_limit.max(1))).await?;
    if pending_head.is_empty() {
        return Ok((0, 0));
    }

    let mut pending_all_for_trigger: Option<Vec<crate::models::Message>> = None;
    let trigger = if pending_head.len() as i64 >= round_limit.max(1) {
        "message_count_limit".to_string()
    } else {
        let all_pending = messages::list_pending_messages(pool, session_id, None).await?;
        let all_texts: Vec<String> = all_pending.iter().map(message_to_summary_block).collect();
        let tokens = estimate_tokens_texts(all_texts.as_slice());
        if tokens >= token_limit.max(500) {
            pending_all_for_trigger = Some(all_pending);
            "token_limit".to_string()
        } else {
            return Ok((0, 0));
        }
    };

    let (selected, pending_before_count) = if trigger == "message_count_limit" {
        let all_pending = messages::list_pending_messages(pool, session_id, None).await?;
        (pending_head, all_pending.len() as i64)
    } else {
        let all_pending = pending_all_for_trigger.unwrap_or_default();
        let pending_before = all_pending.len() as i64;
        (all_pending, pending_before)
    };

    let mut summarizable_messages = Vec::new();
    let mut oversized_messages = Vec::new();
    for message in &selected {
        let tokens = estimate_tokens_text(message_to_summary_block(message).as_str());
        if tokens > token_limit.max(500) {
            oversized_messages.push(message.clone());
        } else {
            summarizable_messages.push(message.clone());
        }
    }

    let selected_ids: Vec<String> = selected.iter().map(|m| m.id.clone()).collect();
    if selected_ids.is_empty() {
        return Ok((0, 0));
    }
    let selected_tokens = selected
        .iter()
        .map(|m| estimate_tokens_text(message_to_summary_block(m).as_str()))
        .sum::<i64>();
    let source_digest = idempotency::digest_from_ids("summary_l0", selected_ids.as_slice())
        .ok_or_else(|| "build summary source digest failed".to_string())?;

    if let Some(existing) =
        summaries::find_summary_by_source_digest(pool, session_id, 0, source_digest.as_str())
            .await?
    {
        if let Err(err) = locks::refresh_job_lock(pool, lock_handle, lease_seconds).await {
            warn!(
                "[MEMORY-SUMMARY-L0] refresh lock failed before reuse: session_id={} error={}",
                session_id, err
            );
        }

        if let Err(err) = memory_sync::sync_memories_from_summary(pool, session_id, &existing).await
        {
            return Err(err);
        }

        let marked = messages::mark_messages_summarized(
            pool,
            session_id,
            selected_ids.as_slice(),
            existing.id.as_str(),
        )
        .await?;
        if marked < selected_ids.len() {
            warn!(
                "[MEMORY-SUMMARY-L0] partial mark on reuse: session_id={} selected={} marked={} summary_id={}",
                session_id,
                selected_ids.len(),
                marked,
                existing.id
            );
        }

        info!(
            "[MEMORY-SUMMARY-L0] reused existing summary by digest: session_id={} digest={} summary_id={} marked={}",
            session_id, source_digest, existing.id, marked
        );
        return Ok((0, marked));
    }

    let job_run = match jobs::create_job_run(
        pool,
        "summary_l0",
        Some(session_id),
        Some(trigger.as_str()),
        selected.len() as i64,
    )
    .await
    {
        Ok(v) => v,
        Err(err) if jobs::is_already_running_error(err.as_str()) => {
            info!(
                "[MEMORY-SUMMARY-L0] skip session already running: session_id={}",
                session_id
            );
            return Ok((0, 0));
        }
        Err(err) => return Err(err),
    };

    if let Err(err) = locks::refresh_job_lock(pool, lock_handle, lease_seconds).await {
        warn!(
            "[MEMORY-SUMMARY-L0] refresh lock failed before llm: session_id={} error={}",
            session_id, err
        );
    }

    let mut overflow_retry_count = 0usize;
    let mut forced_truncated = false;
    let summary_text_result: Result<String, String> = if summarizable_messages.is_empty() {
        Ok(format!(
            "本批次 {} 条消息全部超出 token_limit={}，已仅做标记处理。",
            oversized_messages.len(),
            token_limit
        ))
    } else {
        let blocks: Vec<String> = summarizable_messages
            .iter()
            .map(message_to_summary_block)
            .collect();
        let build = match summarize_texts_with_split(
            ai,
            model_cfg,
            "会话消息总结",
            summary_prompt,
            blocks.as_slice(),
            token_limit,
            target_summary_tokens,
        )
        .await
        {
            Ok(v) => v,
            Err(err) => {
                let _ =
                    summary_support::finish_failed_job_run(pool, job_run.id.as_str(), err.as_str())
                        .await;
                return Err(err);
            }
        };

        let mut text = build.text;
        if build.chunk_count > 1 {
            text.push_str(&format!(
                "\n\n[meta] 该总结由 {} 个分片合并生成。",
                build.chunk_count
            ));
        }
        if build.overflow_retry_count > 0 {
            text.push_str(&format!(
                "\n\n[meta] 发生上下文溢出并自动重试 {} 次后成功。",
                build.overflow_retry_count
            ));
        }
        if build.forced_truncated {
            text.push_str("\n\n[meta] 本次总结触发强制截断兜底，已标记该批次消息为已总结。");
        }
        if !oversized_messages.is_empty() {
            text.push_str(&format!(
                "\n\n[meta] {} 条超长消息未纳入正文总结，但已标记处理。",
                oversized_messages.len()
            ));
        }

        overflow_retry_count = build.overflow_retry_count;
        forced_truncated = build.forced_truncated;
        Ok(text)
    };

    let summary_text = summary_text_result?;

    let mut trigger_type = trigger.clone();
    if !oversized_messages.is_empty() {
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
            "[MEMORY-SUMMARY-L0] refresh lock failed before persist: session_id={} error={}",
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
            source_start_message_id: selected.first().map(|m| m.id.clone()),
            source_end_message_id: selected.last().map(|m| m.id.clone()),
            source_message_count: selected.len() as i64,
            source_estimated_tokens: selected_tokens,
            status: "pending".to_string(),
            error_message: None,
            level: 0,
        },
    )
    .await
    {
        Ok(v) => v,
        Err(err) => {
            let _ = summary_support::finish_failed_job_run(pool, job_run.id.as_str(), err.as_str())
                .await;
            return Err(err);
        }
    };
    let generated = if create_result.inserted { 1 } else { 0 };
    let summary = create_result.summary;

    if let Err(err) = memory_sync::sync_memories_from_summary(pool, session_id, &summary).await {
        let _ =
            summary_support::finish_failed_job_run(pool, job_run.id.as_str(), err.as_str()).await;
        return Err(err);
    }

    if let Err(err) = locks::refresh_job_lock(pool, lock_handle, lease_seconds).await {
        warn!(
            "[MEMORY-SUMMARY-L0] refresh lock failed before mark: session_id={} error={}",
            session_id, err
        );
    }

    let marked = match messages::mark_messages_summarized(
        pool,
        session_id,
        selected_ids.as_slice(),
        summary.id.as_str(),
    )
    .await
    {
        Ok(v) => v,
        Err(err) => {
            let _ = summary_support::finish_failed_job_run(pool, job_run.id.as_str(), err.as_str())
                .await;
            return Err(err);
        }
    };

    if marked < selected_ids.len() {
        warn!(
            "[MEMORY-SUMMARY-L0] partial mark: session_id={} selected={} marked={} summary_id={}",
            session_id,
            selected_ids.len(),
            marked,
            summary.id
        );
    }

    let pending_after_count = match messages::list_pending_messages(pool, session_id, None).await {
        Ok(rows) => Some(rows.len() as i64),
        Err(err) => {
            warn!(
                "[MEMORY-SUMMARY-L0] query pending after mark failed: session_id={} error={}",
                session_id, err
            );
            None
        }
    };

    if let Err(err) = jobs::update_job_run_diagnostics(
        pool,
        job_run.id.as_str(),
        Some(pending_before_count),
        Some(selected.len() as i64),
        Some(marked as i64),
        pending_after_count,
    )
    .await
    {
        warn!(
            "[MEMORY-SUMMARY-L0] update job diagnostics failed: session_id={} job_run_id={} error={}",
            session_id, job_run.id, err
        );
    }

    if let Err(err) =
        jobs::finish_job_run(pool, job_run.id.as_str(), "done", generated as i64, None).await
    {
        warn!(
            "[MEMORY-SUMMARY-L0] finish job run failed: session_id={} job_run_id={} error={}",
            session_id, job_run.id, err
        );
    }

    info!(
        "[MEMORY-SUMMARY-L0] done session_id={} selected={} marked={} generated={} summary_id={}",
        session_id,
        selected.len(),
        marked,
        generated,
        summary.id
    );

    Ok((generated, marked))
}
