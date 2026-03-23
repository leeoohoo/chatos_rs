use std::collections::HashSet;

use tracing::{info, warn};
use uuid::Uuid;

use crate::ai::AiClient;
use crate::db::Db;
use crate::models::{AgentRecall, AiModelConfig};
use crate::repositories::{auth::ADMIN_USER_ID, configs, jobs, memories, summaries};
use crate::repositories::summaries::AgentMemorySummarySource;
use crate::services::summarizer::{estimate_tokens_text, summarize_texts_with_split};

#[derive(Debug, Clone, serde::Serialize)]
pub struct AgentMemoryRunResult {
    pub processed_agents: usize,
    pub summarized_agents: usize,
    pub generated_recalls: usize,
    pub marked_source_summaries: usize,
    pub marked_source_recalls: usize,
    pub failed_agents: usize,
}

#[derive(Debug, Clone)]
struct RecallRollupSelection {
    level: i64,
    selected: Vec<AgentRecall>,
    trigger_reason: &'static str,
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

async fn generate_level0_recall_from_summaries(
    pool: &Db,
    ai: &AiClient,
    user_id: &str,
    agent_id: &str,
    model_cfg: Option<&AiModelConfig>,
    round_limit: i64,
    token_limit: i64,
    target_summary_tokens: i64,
) -> Result<(usize, usize), String> {
    let candidates =
        summaries::list_pending_agent_memory_summaries_by_agent(pool, user_id, agent_id).await?;
    let selected = select_summary_batch(candidates.as_slice(), round_limit, token_limit);
    let Some(selected) = selected else {
        return Ok((0, 0));
    };

    let selected_ids: Vec<String> = selected.iter().map(|item| item.id.clone()).collect();
    let selected_texts: Vec<String> = selected
        .iter()
        .map(|item| {
            format!(
                "[project_id={}][summary_id={}][created_at={}][trigger_type={}]\n{}",
                item.project_id.clone().unwrap_or_else(|| "0".to_string()),
                item.id,
                item.created_at,
                item.trigger_type,
                item.summary_text
            )
        })
        .collect();
    let selected_tokens = selected_texts
        .iter()
        .map(|text| estimate_tokens_text(text.as_str()))
        .sum::<i64>();

    let trigger_reason = if selected.len() as i64 >= round_limit {
        "summary_count_limit"
    } else {
        "token_limit"
    };
    let trigger = format!("agent_memory_l0+{}", trigger_reason);
    let job_run = jobs::create_job_run(
        pool,
        "agent_memory_l0",
        None,
        Some(trigger.as_str()),
        selected.len() as i64,
    )
    .await?;

    let build = match summarize_texts_with_split(
        ai,
        model_cfg,
        "智能体记忆总结 level 0（基于项目总结）",
        selected_texts.as_slice(),
        token_limit,
        target_summary_tokens,
    )
    .await
    {
        Ok(value) => value,
        Err(err) => {
            let _ = finish_failed_job_run(pool, job_run.id.as_str(), err.as_str()).await;
            return Err(err);
        }
    };

    let mut recall_text = build.text;
    if build.chunk_count > 1 {
        recall_text.push_str(&format!(
            "\n\n[meta] 该回忆由 {} 个分片合并。",
            build.chunk_count
        ));
    }
    if build.overflow_retry_count > 0 {
        recall_text.push_str(&format!(
            "\n\n[meta] 发生上下文溢出并自动重试 {} 次后成功。",
            build.overflow_retry_count
        ));
    }

    let recall_key = format!("agent_recall:l0:{}", Uuid::new_v4());
    let _ = memories::upsert_agent_recall(
        pool,
        memories::UpsertAgentRecallInput {
            user_id: user_id.to_string(),
            agent_id: agent_id.to_string(),
            recall_key,
            recall_text,
            level: 0,
            confidence: None,
            last_seen_at: Some(crate::repositories::now_rfc3339()),
        },
    )
    .await?;

    let marked =
        summaries::mark_summaries_agent_memory_summarized(pool, selected_ids.as_slice()).await?;

    if let Err(err) = jobs::finish_job_run(pool, job_run.id.as_str(), "done", 1, None).await {
        warn!(
            "[MEMORY-AGENT-RECALL] finish job run failed: user_id={} agent_id={} job_run_id={} error={}",
            user_id, agent_id, job_run.id, err
        );
    }

    info!(
        "[MEMORY-AGENT-RECALL] l0 done user_id={} agent_id={} selected_summaries={} tokens={} marked_summaries={}",
        user_id,
        agent_id,
        selected.len(),
        selected_tokens,
        marked
    );

    Ok((1, marked))
}

async fn generate_rollup_recall(
    pool: &Db,
    ai: &AiClient,
    user_id: &str,
    agent_id: &str,
    model_cfg: Option<&AiModelConfig>,
    round_limit: i64,
    token_limit: i64,
    target_summary_tokens: i64,
    keep_raw_level0_count: i64,
    max_level: i64,
) -> Result<(usize, usize), String> {
    let selection = select_rollup_batch(
        pool,
        user_id,
        agent_id,
        round_limit,
        token_limit,
        keep_raw_level0_count,
        max_level,
    )
    .await?;

    let Some(selection) = selection else {
        return Ok((0, 0));
    };

    let level = selection.level;
    let target_level = level + 1;
    let selected = selection.selected;

    let mut summarizable = Vec::new();
    let mut oversized = Vec::new();
    for recall in &selected {
        let block = recall_to_rollup_block(recall);
        let tokens = estimate_tokens_text(block.as_str());
        if tokens > token_limit {
            oversized.push(recall.clone());
        } else {
            summarizable.push(block);
        }
    }

    let selected_ids: Vec<String> = selected.iter().map(|item| item.id.clone()).collect();
    let selected_tokens = selected
        .iter()
        .map(|item| estimate_tokens_text(item.recall_text.as_str()))
        .sum::<i64>();

    let trigger = format!(
        "agent_recall_rollup_level_{}_to_{}+{}",
        level, target_level, selection.trigger_reason
    );
    let job_run = jobs::create_job_run(
        pool,
        "agent_memory_rollup",
        None,
        Some(trigger.as_str()),
        selected.len() as i64,
    )
    .await?;

    let recall_text: String = if summarizable.is_empty() {
        format!(
            "本批次 recall level={} 的 {} 条内容全部超出 token_limit={}，仅做层级标记处理。",
            level,
            oversized.len(),
            token_limit
        )
    } else {
        let build = match summarize_texts_with_split(
            ai,
            model_cfg,
            &format!("智能体记忆再总结 level {} -> {}", level, target_level),
            summarizable.as_slice(),
            token_limit,
            target_summary_tokens,
        )
        .await
        {
            Ok(value) => value,
            Err(err) => {
                let _ = finish_failed_job_run(pool, job_run.id.as_str(), err.as_str()).await;
                return Err(err);
            }
        };

        let mut merged = build.text;
        if build.chunk_count > 1 {
            merged.push_str(&format!(
                "\n\n[meta] 该 rollup 回忆由 {} 个分片合并。",
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
            merged.push_str("\n\n[meta] 本次 rollup 触发强制截断兜底，已标记该批次回忆为已 rollup。");
        }
        if !oversized.is_empty() {
            merged.push_str(&format!(
                "\n\n[meta] {} 条超长回忆未纳入正文，但已标记为已 rollup。",
                oversized.len()
            ));
        }

        merged
    };

    let rollup_recall_key = format!("agent_recall:l{}:{}", target_level, Uuid::new_v4());
    let _ = memories::upsert_agent_recall(
        pool,
        memories::UpsertAgentRecallInput {
            user_id: user_id.to_string(),
            agent_id: agent_id.to_string(),
            recall_key: rollup_recall_key.clone(),
            recall_text,
            level: target_level,
            confidence: None,
            last_seen_at: Some(crate::repositories::now_rfc3339()),
        },
    )
    .await?;

    let marked = memories::mark_agent_recalls_rolled_up(
        pool,
        user_id,
        agent_id,
        selected_ids.as_slice(),
        rollup_recall_key.as_str(),
    )
    .await?;

    if let Err(err) = jobs::finish_job_run(pool, job_run.id.as_str(), "done", 1, None).await {
        warn!(
            "[MEMORY-AGENT-RECALL] finish rollup job failed: user_id={} agent_id={} job_run_id={} error={}",
            user_id, agent_id, job_run.id, err
        );
    }

    info!(
        "[MEMORY-AGENT-RECALL] rollup done user_id={} agent_id={} level={}->{} selected={} tokens={} marked={}",
        user_id,
        agent_id,
        level,
        target_level,
        selected.len(),
        selected_tokens,
        marked
    );

    Ok((1, marked))
}

fn recall_to_rollup_block(recall: &AgentRecall) -> String {
    format!(
        "[level={}][recall_key={}][updated_at={}]\n{}",
        recall.level, recall.recall_key, recall.updated_at, recall.recall_text
    )
}

fn select_summary_batch(
    candidates: &[AgentMemorySummarySource],
    round_limit: i64,
    token_limit: i64,
) -> Option<Vec<AgentMemorySummarySource>> {
    if candidates.is_empty() {
        return None;
    }

    if candidates.len() as i64 >= round_limit {
        return Some(candidates.iter().take(round_limit as usize).cloned().collect());
    }

    let token_sum = candidates
        .iter()
        .map(|item| estimate_tokens_text(item.summary_text.as_str()))
        .sum::<i64>();
    if token_sum >= token_limit {
        return Some(candidates.to_vec());
    }

    None
}

async fn select_rollup_batch(
    pool: &Db,
    user_id: &str,
    agent_id: &str,
    round_limit: i64,
    token_limit: i64,
    keep_raw_level0_count: i64,
    max_level: i64,
) -> Result<Option<RecallRollupSelection>, String> {
    for level in 0..max_level {
        let mut candidates =
            memories::list_pending_agent_recalls_by_level(pool, user_id, agent_id, level).await?;

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
            let selected: Vec<AgentRecall> = candidates
                .iter()
                .take(round_limit as usize)
                .cloned()
                .collect();
            return Ok(Some(RecallRollupSelection {
                level,
                selected,
                trigger_reason: "message_count_limit",
            }));
        }

        let token_sum = candidates
            .iter()
            .map(recall_to_rollup_block)
            .map(|text| estimate_tokens_text(text.as_str()))
            .sum::<i64>();
        if token_sum >= token_limit {
            return Ok(Some(RecallRollupSelection {
                level,
                selected: candidates,
                trigger_reason: "token_limit",
            }));
        }
    }

    Ok(None)
}

async fn finish_failed_job_run(pool: &Db, job_run_id: &str, error_message: &str) {
    if let Err(err) = jobs::finish_job_run(pool, job_run_id, "failed", 0, Some(error_message)).await
    {
        warn!(
            "[MEMORY-AGENT-RECALL] mark job failed status failed: job_run_id={} error={}",
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
