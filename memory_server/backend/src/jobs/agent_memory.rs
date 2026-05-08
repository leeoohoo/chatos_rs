use std::collections::HashSet;

use tracing::{info, warn};

use super::job_support;
use crate::config::AppConfig;
use crate::db::Db;
use crate::repositories::{configs, contacts, locks};
use crate::services::memory_engine_client;

#[derive(Debug, Clone, serde::Serialize)]
pub struct AgentMemoryRunResult {
    pub processed_agents: usize,
    pub summarized_agents: usize,
    pub generated_recalls: usize,
    pub marked_source_summaries: usize,
    pub marked_source_recalls: usize,
    pub failed_agents: usize,
}

pub async fn run_once(
    pool: &Db,
    app_config: &AppConfig,
    user_id: &str,
) -> Result<AgentMemoryRunResult, String> {
    let job_config = configs::get_effective_agent_memory_job_config(pool, user_id).await?;
    if job_config.enabled != 1 {
        return Ok(AgentMemoryRunResult {
            processed_agents: 0,
            summarized_agents: 0,
            generated_recalls: 0,
            marked_source_summaries: 0,
            marked_source_recalls: 0,
            failed_agents: 0,
        });
    }

    let max_agents_per_tick = job_config.max_agents_per_tick.max(1);
    let summary_agents = memory_engine_client::list_agent_ids_with_pending_agent_memory_by_user(
        app_config,
        pool,
        user_id,
        max_agents_per_tick,
    )
    .await?;
    let recall_agents = list_agent_ids_with_pending_recall_rollup(
        pool,
        app_config,
        user_id,
        job_config.max_level,
        max_agents_per_tick,
    )
    .await?;

    let mut seen = HashSet::new();
    let mut agent_ids = Vec::new();
    for agent_id in summary_agents.into_iter().chain(recall_agents.into_iter()) {
        if seen.insert(agent_id.clone()) {
            agent_ids.push(agent_id);
            if agent_ids.len() >= max_agents_per_tick as usize {
                break;
            }
        }
    }

    let mut out = AgentMemoryRunResult {
        processed_agents: agent_ids.len(),
        summarized_agents: 0,
        generated_recalls: 0,
        marked_source_summaries: 0,
        marked_source_recalls: 0,
        failed_agents: 0,
    };

    for agent_id in agent_ids {
        match process_agent(
            pool,
            app_config,
            user_id,
            agent_id.as_str(),
            job_config.summary_prompt.as_deref(),
            job_config.round_limit,
            job_config.token_limit,
            job_config.target_summary_tokens,
            job_config.keep_raw_level0_count,
            job_config.max_level,
        )
        .await
        {
            Ok((generated, marked_summaries, marked_recalls)) => {
                if generated > 0 {
                    out.summarized_agents += 1;
                }
                out.generated_recalls += generated;
                out.marked_source_summaries += marked_summaries;
                out.marked_source_recalls += marked_recalls;
            }
            Err(err) => {
                out.failed_agents += 1;
                warn!(
                    "[MEMORY-AGENT-RECALL] process failed: user_id={} agent_id={} error={}",
                    user_id, agent_id, err
                );
            }
        }
    }

    Ok(out)
}

#[allow(clippy::too_many_arguments)]
async fn process_agent(
    pool: &Db,
    config: &AppConfig,
    user_id: &str,
    agent_id: &str,
    summary_prompt: Option<&str>,
    round_limit: i64,
    token_limit: i64,
    target_summary_tokens: i64,
    keep_raw_level0_count: i64,
    max_level: i64,
) -> Result<(usize, usize, usize), String> {
    let lease_seconds = job_support::resolve_lock_lease_seconds();
    let lock_key = format!("agent_memory:{}:{}", user_id, agent_id);
    let Some(lock_handle) =
        locks::try_acquire_job_lock(pool, lock_key.as_str(), lease_seconds).await?
    else {
        info!(
            "[MEMORY-AGENT-RECALL] skip agent lock busy: user_id={} agent_id={}",
            user_id, agent_id
        );
        return Ok((0, 0, 0));
    };

    let result = process_agent_locked(
        pool,
        config,
        user_id,
        agent_id,
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
            "[MEMORY-AGENT-RECALL] release lock failed: user_id={} agent_id={} key={} error={}",
            user_id, agent_id, lock_handle.lock_key, err
        );
    }

    result
}

#[allow(clippy::too_many_arguments)]
async fn process_agent_locked(
    pool: &Db,
    config: &AppConfig,
    user_id: &str,
    agent_id: &str,
    summary_prompt: Option<&str>,
    round_limit: i64,
    token_limit: i64,
    target_summary_tokens: i64,
    keep_raw_level0_count: i64,
    max_level: i64,
    lock_handle: &locks::JobLockHandle,
    lease_seconds: i64,
) -> Result<(usize, usize, usize), String> {
    if let Err(err) = locks::refresh_job_lock(pool, lock_handle, lease_seconds).await {
        warn!(
            "[MEMORY-AGENT-RECALL] refresh lock failed before engine run: user_id={} agent_id={} error={}",
            user_id, agent_id, err
        );
    }

    let result = memory_engine_client::run_agent_recall_job(
        config,
        user_id,
        agent_id,
        summary_prompt,
        round_limit.max(1),
        token_limit.max(500),
        target_summary_tokens.max(200),
        keep_raw_level0_count.max(0),
        max_level.max(1),
    )
    .await?;

    Ok((
        result.generated_memories,
        result.marked_source_summaries,
        result.marked_source_memories,
    ))
}

async fn list_agent_ids_with_pending_recall_rollup(
    pool: &Db,
    config: &AppConfig,
    user_id: &str,
    max_level: i64,
    max_agents_per_tick: i64,
) -> Result<Vec<String>, String> {
    let contacts = contacts::list_contacts(pool, user_id, Some("active"), 5_000, 0).await?;
    let mut out = Vec::new();
    for contact in contacts {
        let has_pending = memory_engine_client::has_pending_agent_recalls_before_level(
            config,
            user_id,
            contact.agent_id.as_str(),
            max_level,
        )
        .await?;
        if has_pending {
            out.push(contact.agent_id);
            if out.len() >= max_agents_per_tick as usize {
                break;
            }
        }
    }

    Ok(out)
}
