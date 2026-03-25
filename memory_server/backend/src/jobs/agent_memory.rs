use std::collections::HashSet;

use tracing::warn;

use super::agent_memory_generation::{
    generate_level0_recall_from_summaries, generate_rollup_recall,
};
use super::agent_memory_support::resolve_model_config;
use crate::ai::AiClient;
use crate::db::Db;
use crate::models::AiModelConfig;
use crate::repositories::{configs, memories, summaries};

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
    ai: &AiClient,
    user_id: &str,
) -> Result<AgentMemoryRunResult, String> {
    let config = configs::get_effective_agent_memory_job_config(pool, user_id).await?;
    if config.enabled != 1 {
        return Ok(AgentMemoryRunResult {
            processed_agents: 0,
            summarized_agents: 0,
            generated_recalls: 0,
            marked_source_summaries: 0,
            marked_source_recalls: 0,
            failed_agents: 0,
        });
    }

    let model_cfg =
        resolve_model_config(pool, user_id, config.summary_model_config_id.as_deref()).await?;

    let max_agents_per_tick = config.max_agents_per_tick.max(1);
    let summary_agents = summaries::list_agent_ids_with_pending_agent_memory_by_user(
        pool,
        user_id,
        max_agents_per_tick,
    )
    .await?;
    let recall_agents = memories::list_agent_ids_with_pending_recall_rollup_by_user(
        pool,
        user_id,
        config.max_level,
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
            ai,
            user_id,
            agent_id.as_str(),
            model_cfg.as_ref(),
            config.summary_prompt.as_deref(),
            config.round_limit,
            config.token_limit,
            config.target_summary_tokens,
            config.keep_raw_level0_count,
            config.max_level,
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

async fn process_agent(
    pool: &Db,
    ai: &AiClient,
    user_id: &str,
    agent_id: &str,
    model_cfg: Option<&AiModelConfig>,
    summary_prompt: Option<&str>,
    round_limit: i64,
    token_limit: i64,
    target_summary_tokens: i64,
    keep_raw_level0_count: i64,
    max_level: i64,
) -> Result<(usize, usize, usize), String> {
    let mut generated_recalls = 0usize;
    let mut marked_source_summaries = 0usize;
    let mut marked_source_recalls = 0usize;

    let (generated_l0, marked_summaries) = generate_level0_recall_from_summaries(
        pool,
        ai,
        user_id,
        agent_id,
        model_cfg,
        summary_prompt,
        round_limit.max(1),
        token_limit.max(500),
        target_summary_tokens.max(200),
    )
    .await?;
    generated_recalls += generated_l0;
    marked_source_summaries += marked_summaries;

    let (generated_rollup, marked_recalls) = generate_rollup_recall(
        pool,
        ai,
        user_id,
        agent_id,
        model_cfg,
        summary_prompt,
        round_limit.max(1),
        token_limit.max(500),
        target_summary_tokens.max(200),
        keep_raw_level0_count.max(0),
        max_level.max(1),
    )
    .await?;
    generated_recalls += generated_rollup;
    marked_source_recalls += marked_recalls;

    Ok((
        generated_recalls,
        marked_source_summaries,
        marked_source_recalls,
    ))
}
