use std::sync::Arc;

use tokio::sync::Semaphore;
use tokio::task::JoinSet;
use tracing::{info, warn};

use crate::ai::AiClient;
use crate::db::Db;
use crate::models::{AiModelConfig, CreateSummaryInput};
use crate::repositories::{auth::ADMIN_USER_ID, configs, jobs, messages, summaries};
use crate::services::summarizer::{
    estimate_tokens_text, estimate_tokens_texts, message_to_summary_block, summarize_texts_with_split,
};

#[derive(Debug, Clone, serde::Serialize)]
pub struct SummaryRunResult {
    pub processed_sessions: usize,
    pub summarized_sessions: usize,
    pub generated_summaries: usize,
    pub marked_messages: usize,
    pub failed_sessions: usize,
}

pub async fn run_once(pool: &Db, ai: &AiClient, user_id: &str) -> Result<SummaryRunResult, String> {
    let config = configs::get_summary_job_config(pool, user_id).await?;
    if config.enabled != 1 {
        return Ok(SummaryRunResult {
            processed_sessions: 0,
            summarized_sessions: 0,
            generated_summaries: 0,
            marked_messages: 0,
            failed_sessions: 0,
        });
    }

    let model_cfg = resolve_model_config(pool, user_id, config.summary_model_config_id.as_deref()).await?;
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

    let concurrency = resolve_session_concurrency(
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
        let user_id = user_id.to_string();
        let model_name = model_name.clone();
        let model_cfg = model_cfg.clone();
        let round_limit = config.round_limit;
        let token_limit = config.token_limit;
        let target_summary_tokens = config.target_summary_tokens;

        join_set.spawn(async move {
            let _permit = permit;
            let result = process_session(
                &pool,
                &ai,
                user_id.as_str(),
                session_id.as_str(),
                &model_name,
                model_cfg.as_ref(),
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
    _user_id: &str,
    session_id: &str,
    model_name: &str,
    model_cfg: Option<&AiModelConfig>,
    round_limit: i64,
    token_limit: i64,
    target_summary_tokens: i64,
) -> Result<(usize, usize), String> {
    let pending_head = messages::list_pending_messages(pool, session_id, Some(round_limit.max(1))).await?;
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

    let job_run = jobs::create_job_run(
        pool,
        "summary_l0",
        Some(session_id),
        Some(trigger.as_str()),
        selected.len() as i64,
    )
    .await?;

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
        .await {
            Ok(v) => v,
            Err(err) => {
                let _ = finish_failed_job_run(pool, job_run.id.as_str(), err.as_str()).await;
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
            text.push_str(
                "\n\n[meta] 本次总结触发强制截断兜底，已标记该批次消息为已总结。",
            );
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
    .await {
        Ok(v) => v,
        Err(err) => {
            let _ = finish_failed_job_run(pool, job_run.id.as_str(), err.as_str()).await;
            return Err(err);
        }
    };

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
            let _ = finish_failed_job_run(pool, job_run.id.as_str(), err.as_str()).await;
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

async fn finish_failed_job_run(pool: &Db, job_run_id: &str, error_message: &str) {
    if let Err(err) = jobs::finish_job_run(pool, job_run_id, "failed", 0, Some(error_message)).await
    {
        warn!(
            "[MEMORY-SUMMARY-L0] mark job failed status failed: job_run_id={} error={}",
            job_run_id, err
        );
    }
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

pub async fn run_once_for_session(
    pool: &Db,
    ai: &AiClient,
    user_id: &str,
    session_id: &str,
) -> Result<(), String> {
    let config = configs::get_summary_job_config(pool, user_id).await?;
    if config.enabled != 1 {
        return Ok(());
    }

    let model_cfg = resolve_model_config(pool, user_id, config.summary_model_config_id.as_deref()).await?;
    let model_name = model_cfg
        .as_ref()
        .map(|m| m.model.clone())
        .unwrap_or_else(|| "local-fallback".to_string());

    let _ = process_session(
        pool,
        ai,
        user_id,
        session_id,
        &model_name,
        model_cfg.as_ref(),
        config.round_limit,
        config.token_limit,
        config.target_summary_tokens,
    )
    .await?;

    Ok(())
}
