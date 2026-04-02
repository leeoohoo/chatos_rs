use std::sync::Arc;

use tokio::sync::Semaphore;
use tokio::task::JoinSet;
use tracing::{info, warn};

use crate::ai::AiClient;
use crate::db::Db;
use crate::models::{AiModelConfig, CreateTaskExecutionSummaryInput, TaskExecutionScope};
use crate::repositories::{configs, jobs, locks, task_execution_messages, task_execution_summaries};
use crate::services::summarizer::{
    estimate_tokens_text, estimate_tokens_texts, summarize_texts_with_split,
};

use super::{idempotency, job_support, summary_support};

#[derive(Debug, Clone, serde::Serialize)]
pub struct TaskExecutionSummaryRunResult {
    pub processed_scopes: usize,
    pub summarized_scopes: usize,
    pub generated_summaries: usize,
    pub marked_messages: usize,
    pub failed_scopes: usize,
}

pub async fn run_once(
    pool: &Db,
    ai: &AiClient,
    user_id: &str,
) -> Result<TaskExecutionSummaryRunResult, String> {
    let config = configs::get_effective_summary_job_config(pool, user_id).await?;
    if config.enabled != 1 {
        return Ok(TaskExecutionSummaryRunResult {
            processed_scopes: 0,
            summarized_scopes: 0,
            generated_summaries: 0,
            marked_messages: 0,
            failed_scopes: 0,
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

    let scopes = task_execution_messages::list_pending_scopes_by_user(
        pool,
        user_id,
        config.max_sessions_per_tick,
    )
    .await?;

    let mut out = TaskExecutionSummaryRunResult {
        processed_scopes: scopes.len(),
        summarized_scopes: 0,
        generated_summaries: 0,
        marked_messages: 0,
        failed_scopes: 0,
    };
    if scopes.is_empty() {
        return Ok(out);
    }

    let concurrency = job_support::resolve_tick_concurrency(
        config.max_sessions_per_tick,
        "MEMORY_SERVER_TASK_EXECUTION_SUMMARY_CONCURRENCY",
        2,
    );
    info!(
        "[MEMORY-TASK-EXEC-SUMMARY] run_once user_id={} scopes={} concurrency={}",
        user_id,
        scopes.len(),
        concurrency
    );

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
            )
            .await;
            (scope, result)
        });
    }

    while let Some(joined) = join_set.join_next().await {
        match joined {
            Ok((_, Ok((generated, marked)))) => {
                if generated > 0 {
                    out.summarized_scopes += 1;
                }
                out.generated_summaries += generated;
                out.marked_messages += marked;
            }
            Ok((scope, Err(err))) => {
                out.failed_scopes += 1;
                warn!(
                    "[MEMORY-TASK-EXEC-SUMMARY] process failed: user_id={} scope_key={} error={}",
                    user_id, scope.scope_key, err
                );
            }
            Err(err) => {
                out.failed_scopes += 1;
                warn!(
                    "[MEMORY-TASK-EXEC-SUMMARY] join failed: user_id={} error={}",
                    user_id, err
                );
            }
        }
    }

    Ok(out)
}

#[allow(clippy::too_many_arguments)]
async fn process_scope(
    pool: &Db,
    ai: &AiClient,
    scope: &TaskExecutionScope,
    model_name: &str,
    model_cfg: Option<&AiModelConfig>,
    summary_prompt: Option<&str>,
    round_limit: i64,
    token_limit: i64,
    target_summary_tokens: i64,
) -> Result<(usize, usize), String> {
    let lease_seconds = job_support::resolve_lock_lease_seconds();
    let lock_key = format!("task_exec_summary_l0:{}", scope.scope_key);
    let Some(lock_handle) =
        locks::try_acquire_job_lock(pool, lock_key.as_str(), lease_seconds).await?
    else {
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
        &lock_handle,
        lease_seconds,
    )
    .await;

    let _ = locks::release_job_lock(pool, &lock_handle).await;
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
    lock_handle: &locks::JobLockHandle,
    lease_seconds: i64,
) -> Result<(usize, usize), String> {
    let pending_head = task_execution_messages::list_pending_messages(
        pool,
        scope.user_id.as_str(),
        scope.contact_agent_id.as_str(),
        scope.project_id.as_str(),
        Some(round_limit.max(1)),
    )
    .await?;
    if pending_head.is_empty() {
        return Ok((0, 0));
    }

    let mut pending_all_for_trigger: Option<Vec<crate::models::TaskExecutionMessage>> = None;
    let trigger = if pending_head.len() as i64 >= round_limit.max(1) {
        "message_count_limit".to_string()
    } else {
        let all_pending = task_execution_messages::list_pending_messages(
            pool,
            scope.user_id.as_str(),
            scope.contact_agent_id.as_str(),
            scope.project_id.as_str(),
            None,
        )
        .await?;
        let all_texts: Vec<String> = all_pending.iter().map(task_message_to_summary_block).collect();
        let tokens = estimate_tokens_texts(all_texts.as_slice());
        if tokens >= token_limit.max(500) {
            pending_all_for_trigger = Some(all_pending);
            "token_limit".to_string()
        } else {
            return Ok((0, 0));
        }
    };

    let (selected, pending_before_count) = if trigger == "message_count_limit" {
        let all_pending = task_execution_messages::list_pending_messages(
            pool,
            scope.user_id.as_str(),
            scope.contact_agent_id.as_str(),
            scope.project_id.as_str(),
            None,
        )
        .await?;
        (pending_head, all_pending.len() as i64)
    } else {
        let all_pending = pending_all_for_trigger.unwrap_or_default();
        let pending_before = all_pending.len() as i64;
        (all_pending, pending_before)
    };

    let mut summarizable_messages = Vec::new();
    let mut oversized_messages = Vec::new();
    for message in &selected {
        let tokens = estimate_tokens_text(task_message_to_summary_block(message).as_str());
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
        .map(|m| estimate_tokens_text(task_message_to_summary_block(m).as_str()))
        .sum::<i64>();
    let source_digest =
        idempotency::digest_from_ids("task_exec_summary_l0", selected_ids.as_slice())
            .ok_or_else(|| "build task execution summary source digest failed".to_string())?;

    if let Some(existing) = task_execution_summaries::find_summary_by_source_digest(
        pool,
        scope.user_id.as_str(),
        scope.contact_agent_id.as_str(),
        scope.project_id.as_str(),
        0,
        source_digest.as_str(),
    )
    .await?
    {
        let _ = locks::refresh_job_lock(pool, lock_handle, lease_seconds).await;
        let marked = task_execution_messages::mark_messages_summarized(
            pool,
            scope.user_id.as_str(),
            scope.contact_agent_id.as_str(),
            scope.project_id.as_str(),
            selected_ids.as_slice(),
            existing.id.as_str(),
        )
        .await?;
        return Ok((0, marked));
    }

    let job_run = match jobs::create_job_run(
        pool,
        "task_execution_summary_l0",
        Some(scope.scope_key.as_str()),
        Some(trigger.as_str()),
        selected.len() as i64,
    )
    .await
    {
        Ok(v) => v,
        Err(err) if jobs::is_already_running_error(err.as_str()) => return Ok((0, 0)),
        Err(err) => return Err(err),
    };

    let _ = locks::refresh_job_lock(pool, lock_handle, lease_seconds).await;
    let summary_text = if summarizable_messages.is_empty() {
        format!(
            "本批次 {} 条后台任务执行消息均超过 token_limit={}，已仅做标记处理。",
            oversized_messages.len(),
            token_limit
        )
    } else {
        let blocks = summarizable_messages
            .iter()
            .map(task_message_to_summary_block)
            .collect::<Vec<_>>();
        let build = match summarize_texts_with_split(
            ai,
            model_cfg,
            "后台任务执行消息总结",
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
        build.text
    };

    let _ = locks::refresh_job_lock(pool, lock_handle, lease_seconds).await;
    let create_result = match task_execution_summaries::create_summary(
        pool,
        CreateTaskExecutionSummaryInput {
            user_id: scope.user_id.clone(),
            contact_agent_id: scope.contact_agent_id.clone(),
            project_id: scope.project_id.clone(),
            source_digest: Some(source_digest),
            summary_text,
            summary_model: model_name.to_string(),
            trigger_type: trigger,
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
    let marked = match task_execution_messages::mark_messages_summarized(
        pool,
        scope.user_id.as_str(),
        scope.contact_agent_id.as_str(),
        scope.project_id.as_str(),
        selected_ids.as_slice(),
        create_result.summary.id.as_str(),
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

    let pending_after_count = task_execution_messages::list_pending_messages(
        pool,
        scope.user_id.as_str(),
        scope.contact_agent_id.as_str(),
        scope.project_id.as_str(),
        None,
    )
    .await
    .ok()
    .map(|rows| rows.len() as i64);

    let _ = jobs::update_job_run_diagnostics(
        pool,
        job_run.id.as_str(),
        Some(pending_before_count),
        Some(selected.len() as i64),
        Some(marked as i64),
        pending_after_count,
    )
    .await;
    let _ = jobs::finish_job_run(pool, job_run.id.as_str(), "done", generated as i64, None).await;
    Ok((generated, marked))
}

fn task_message_to_summary_block(message: &crate::models::TaskExecutionMessage) -> String {
    let mut extras = Vec::new();
    if let Some(task_id) = message.task_id.as_deref().map(str::trim).filter(|v| !v.is_empty()) {
        extras.push(format!("task_id={task_id}"));
    }
    if let Some(source_session_id) = message
        .source_session_id
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
    {
        extras.push(format!("source_session_id={source_session_id}"));
    }
    let extra_suffix = if extras.is_empty() {
        String::new()
    } else {
        format!(" [{}]", extras.join(", "))
    };
    format!(
        "[{}][{}{}]\n{}",
        message.created_at, message.role, extra_suffix, message.content
    )
}
