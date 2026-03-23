use tracing::{info, warn};

use crate::ai::AiClient;
use crate::db::Db;
use crate::models::{AiModelConfig, CreateSummaryInput};
use crate::repositories::{jobs, messages, summaries};
use crate::services::summarizer::{
    estimate_tokens_text, estimate_tokens_texts, message_to_summary_block,
    summarize_texts_with_split,
};

use super::{memory_sync, summary_support};

pub(crate) async fn process_session(
    pool: &Db,
    ai: &AiClient,
    session_id: &str,
    model_name: &str,
    model_cfg: Option<&AiModelConfig>,
    round_limit: i64,
    token_limit: i64,
    target_summary_tokens: i64,
) -> Result<(usize, usize), String> {
    let pending_head =
        messages::list_pending_messages(pool, session_id, Some(round_limit.max(1))).await?;
    if pending_head.is_empty() {
        return Ok((0, 0));
    }

    let trigger = if pending_head.len() as i64 >= round_limit.max(1) {
        "message_count_limit".to_string()
    } else {
        let all_pending = messages::list_pending_messages(pool, session_id, None).await?;
        let all_texts: Vec<String> = all_pending.iter().map(message_to_summary_block).collect();
        let tokens = estimate_tokens_texts(all_texts.as_slice());
        if tokens >= token_limit.max(500) {
            "token_limit".to_string()
        } else {
            return Ok((0, 0));
        }
    };

    let selected = if trigger == "message_count_limit" {
        pending_head
    } else {
        messages::list_pending_messages(pool, session_id, None).await?
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

    let summary = match summaries::create_summary(
        pool,
        CreateSummaryInput {
            session_id: session_id.to_string(),
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

    if let Err(err) = memory_sync::sync_memories_from_summary(pool, session_id, &summary).await {
        let _ =
            summary_support::finish_failed_job_run(pool, job_run.id.as_str(), err.as_str()).await;
        return Err(err);
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

    if let Err(err) = jobs::finish_job_run(pool, job_run.id.as_str(), "done", 1, None).await {
        warn!(
            "[MEMORY-SUMMARY-L0] finish job run failed: session_id={} job_run_id={} error={}",
            session_id, job_run.id, err
        );
    }

    info!(
        "[MEMORY-SUMMARY-L0] done session_id={} selected={} marked={} summary_id={}",
        session_id,
        selected.len(),
        marked,
        summary.id
    );

    Ok((1, marked))
}
