use tracing::{info, warn};

use crate::ai::AiClient;
use crate::db::Db;
use crate::models::AiModelConfig;
use crate::repositories::{jobs, memories, summaries, task_result_briefs};
use crate::services::summarizer::{estimate_tokens_text, summarize_texts_with_split};

use super::agent_memory_support::{
    agent_memory_source_digest_id, agent_memory_source_to_text, aggregate_project_ids_from_sources,
    aggregate_task_ids_from_sources, finish_failed_job_run, recall_to_rollup_block,
    resolve_source_kind, select_rollup_batch, select_source_batch, AgentMemorySourceItem,
};
use super::idempotency;

pub(crate) async fn generate_level0_recall_from_summaries(
    pool: &Db,
    ai: &AiClient,
    user_id: &str,
    agent_id: &str,
    model_cfg: Option<&AiModelConfig>,
    summary_prompt: Option<&str>,
    round_limit: i64,
    token_limit: i64,
    target_summary_tokens: i64,
) -> Result<(usize, usize), String> {
    let mut candidates =
        summaries::list_pending_agent_memory_summaries_by_agent(pool, user_id, agent_id)
            .await?
            .into_iter()
            .map(AgentMemorySourceItem::ChatSummary)
            .collect::<Vec<_>>();
    candidates.extend(
        task_result_briefs::list_pending_task_result_briefs_by_agent(pool, user_id, agent_id)
            .await?
            .into_iter()
            .map(AgentMemorySourceItem::TaskResultBrief),
    );
    candidates.sort_by(|a, b| source_sort_key(a).cmp(&source_sort_key(b)));

    let selected = select_source_batch(candidates.as_slice(), round_limit, token_limit);
    let Some(selected) = selected else {
        return Ok((0, 0));
    };

    let selected_ids: Vec<String> = selected.iter().map(agent_memory_source_digest_id).collect();
    let selected_texts: Vec<String> = selected.iter().map(agent_memory_source_to_text).collect();
    let selected_tokens = selected_texts
        .iter()
        .map(|text| estimate_tokens_text(text.as_str()))
        .sum::<i64>();
    let source_digest = idempotency::digest_from_ids("agent_recall_l0", selected_ids.as_slice())
        .ok_or_else(|| "build agent l0 source digest failed".to_string())?;

    if let Some(existing) = memories::find_agent_recall_by_source_digest(
        pool,
        user_id,
        agent_id,
        0,
        source_digest.as_str(),
    )
    .await?
    {
        let marked = mark_agent_memory_sources_summarized(pool, selected.as_slice()).await?;
        if marked < selected_ids.len() {
            warn!(
                "[MEMORY-AGENT-RECALL] partial l0 mark on reuse: user_id={} agent_id={} selected={} marked={} recall_id={}",
                user_id,
                agent_id,
                selected_ids.len(),
                marked,
                existing.id
            );
        }
        info!(
            "[MEMORY-AGENT-RECALL] reused l0 recall by digest: user_id={} agent_id={} digest={} recall_key={} marked={}",
            user_id, agent_id, source_digest, existing.recall_key, marked
        );
        return Ok((0, marked));
    }

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
        summary_prompt,
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

    let recall_key = format!("agent_recall:l0:{}", source_digest);
    let _ = memories::upsert_agent_recall(
        pool,
        memories::UpsertAgentRecallInput {
            user_id: user_id.to_string(),
            agent_id: agent_id.to_string(),
            recall_key,
            source_digest: Some(source_digest.clone()),
            recall_text,
            level: 0,
            source_kind: resolve_source_kind(selected.as_slice()),
            source_scope_kind: Some("agent".to_string()),
            contact_agent_id: Some(agent_id.to_string()),
            project_ids: aggregate_project_ids_from_sources(selected.as_slice()),
            task_ids: aggregate_task_ids_from_sources(selected.as_slice()),
            confidence: None,
            last_seen_at: Some(crate::repositories::now_rfc3339()),
        },
    )
    .await?;

    let marked = mark_agent_memory_sources_summarized(pool, selected.as_slice()).await?;
    if marked < selected_ids.len() {
        warn!(
            "[MEMORY-AGENT-RECALL] partial l0 mark: user_id={} agent_id={} selected={} marked={}",
            user_id,
            agent_id,
            selected_ids.len(),
            marked
        );
    }

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

pub(crate) async fn generate_rollup_recall(
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
    let digest_namespace = format!("agent_recall_rollup:l{}->{}", level, target_level);
    let source_digest =
        idempotency::digest_from_ids(digest_namespace.as_str(), selected_ids.as_slice())
            .ok_or_else(|| "build agent rollup source digest failed".to_string())?;

    if let Some(existing) = memories::find_agent_recall_by_source_digest(
        pool,
        user_id,
        agent_id,
        target_level,
        source_digest.as_str(),
    )
    .await?
    {
        let marked = memories::mark_agent_recalls_rolled_up(
            pool,
            user_id,
            agent_id,
            selected_ids.as_slice(),
            existing.recall_key.as_str(),
        )
        .await?;
        if marked < selected_ids.len() {
            warn!(
                "[MEMORY-AGENT-RECALL] partial rollup mark on reuse: user_id={} agent_id={} level={}->{} selected={} marked={} recall_id={}",
                user_id,
                agent_id,
                level,
                target_level,
                selected_ids.len(),
                marked,
                existing.id
            );
        }
        info!(
            "[MEMORY-AGENT-RECALL] reused rollup recall by digest: user_id={} agent_id={} level={}->{} digest={} recall_key={} marked={}",
            user_id, agent_id, level, target_level, source_digest, existing.recall_key, marked
        );
        return Ok((0, marked));
    }

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
            summary_prompt,
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
            merged
                .push_str("\n\n[meta] 本次 rollup 触发强制截断兜底，已标记该批次回忆为已 rollup。");
        }
        if !oversized.is_empty() {
            merged.push_str(&format!(
                "\n\n[meta] {} 条超长回忆未纳入正文，但已标记为已 rollup。",
                oversized.len()
            ));
        }

        merged
    };

    let rollup_recall_key = format!("agent_recall:l{}:{}", target_level, source_digest);
    let _ = memories::upsert_agent_recall(
        pool,
        memories::UpsertAgentRecallInput {
            user_id: user_id.to_string(),
            agent_id: agent_id.to_string(),
            recall_key: rollup_recall_key.clone(),
            source_digest: Some(source_digest.clone()),
            recall_text,
            level: target_level,
            source_kind: Some("agent_recall_rollup".to_string()),
            source_scope_kind: Some("agent".to_string()),
            contact_agent_id: Some(agent_id.to_string()),
            project_ids: aggregate_project_ids_from_recalls(selected.as_slice()),
            task_ids: aggregate_task_ids_from_recalls(selected.as_slice()),
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
    if marked < selected_ids.len() {
        warn!(
            "[MEMORY-AGENT-RECALL] partial rollup mark: user_id={} agent_id={} level={}->{} selected={} marked={}",
            user_id,
            agent_id,
            level,
            target_level,
            selected_ids.len(),
            marked
        );
    }

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

fn source_sort_key(item: &AgentMemorySourceItem) -> (String, String) {
    match item {
        AgentMemorySourceItem::ChatSummary(item) => (item.created_at.clone(), item.id.clone()),
        AgentMemorySourceItem::TaskResultBrief(item) => (
            item.finished_at
                .clone()
                .unwrap_or_else(|| item.updated_at.clone()),
            item.id.clone(),
        ),
    }
}

async fn mark_agent_memory_sources_summarized(
    pool: &Db,
    sources: &[AgentMemorySourceItem],
) -> Result<usize, String> {
    let mut summary_ids = Vec::new();
    let mut brief_ids = Vec::new();
    for item in sources {
        match item {
            AgentMemorySourceItem::ChatSummary(item) => summary_ids.push(item.id.clone()),
            AgentMemorySourceItem::TaskResultBrief(item) => brief_ids.push(item.id.clone()),
        }
    }

    let marked_summaries =
        summaries::mark_summaries_agent_memory_summarized(pool, summary_ids.as_slice()).await?;
    let marked_briefs = task_result_briefs::mark_task_result_briefs_agent_memory_summarized(
        pool,
        brief_ids.as_slice(),
    )
    .await?;
    Ok(marked_summaries + marked_briefs)
}

fn aggregate_project_ids_from_recalls(selected: &[crate::models::AgentRecall]) -> Vec<String> {
    let mut values = std::collections::BTreeSet::new();
    for item in selected {
        for project_id in &item.project_ids {
            values.insert(project_id.clone());
        }
    }
    values.into_iter().collect()
}

fn aggregate_task_ids_from_recalls(selected: &[crate::models::AgentRecall]) -> Vec<String> {
    let mut values = std::collections::BTreeSet::new();
    for item in selected {
        for task_id in &item.task_ids {
            values.insert(task_id.clone());
        }
    }
    values.into_iter().collect()
}
