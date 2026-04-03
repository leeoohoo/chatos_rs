use super::job_support;
use crate::db::Db;
use crate::models::AgentRecall;
use crate::models::TaskResultBrief;
use crate::repositories::memories;
use crate::repositories::summaries::AgentMemorySummarySource;
use crate::services::summarizer::estimate_tokens_text;
use std::collections::BTreeSet;

#[derive(Debug, Clone)]
pub(crate) enum AgentMemorySourceItem {
    ChatSummary(AgentMemorySummarySource),
    TaskResultBrief(TaskResultBrief),
}

#[derive(Debug, Clone)]
pub(crate) struct RecallRollupSelection {
    pub(crate) level: i64,
    pub(crate) selected: Vec<AgentRecall>,
    pub(crate) trigger_reason: &'static str,
}

pub(crate) fn recall_to_rollup_block(recall: &AgentRecall) -> String {
    format!(
        "[level={}][recall_key={}][updated_at={}]\n{}",
        recall.level, recall.recall_key, recall.updated_at, recall.recall_text
    )
}

pub(crate) fn select_source_batch(
    candidates: &[AgentMemorySourceItem],
    round_limit: i64,
    token_limit: i64,
) -> Option<Vec<AgentMemorySourceItem>> {
    if candidates.is_empty() {
        return None;
    }

    if candidates.len() as i64 >= round_limit {
        return Some(
            candidates
                .iter()
                .take(round_limit as usize)
                .cloned()
                .collect(),
        );
    }

    let token_sum = candidates
        .iter()
        .map(agent_memory_source_to_text)
        .map(|text| estimate_tokens_text(text.as_str()))
        .sum::<i64>();
    if token_sum >= token_limit {
        return Some(candidates.to_vec());
    }

    None
}

pub(crate) fn agent_memory_source_to_text(item: &AgentMemorySourceItem) -> String {
    match item {
        AgentMemorySourceItem::ChatSummary(item) => format!(
            "[source_kind=chat_summary][project_id={}][summary_id={}][created_at={}][trigger_type={}]\n{}",
            item.project_id.clone().unwrap_or_else(|| "0".to_string()),
            item.id,
            item.created_at,
            item.trigger_type,
            item.summary_text
        ),
        AgentMemorySourceItem::TaskResultBrief(item) => format!(
            "[source_kind=task_result][project_id={}][task_id={}][task_status={}][finished_at={}]\n{}",
            item.project_id,
            item.task_id,
            item.task_status,
            item.finished_at
                .clone()
                .unwrap_or_else(|| item.updated_at.clone()),
            item.result_summary
        ),
    }
}

pub(crate) fn agent_memory_source_digest_id(item: &AgentMemorySourceItem) -> String {
    match item {
        AgentMemorySourceItem::ChatSummary(item) => format!("chat_summary:{}", item.id),
        AgentMemorySourceItem::TaskResultBrief(item) => format!("task_result:{}", item.id),
    }
}

pub(crate) fn aggregate_project_ids_from_sources(
    candidates: &[AgentMemorySourceItem],
) -> Vec<String> {
    let mut project_ids = BTreeSet::new();
    for item in candidates {
        match item {
            AgentMemorySourceItem::ChatSummary(item) => {
                project_ids.insert(item.project_id.clone().unwrap_or_else(|| "0".to_string()));
            }
            AgentMemorySourceItem::TaskResultBrief(item) => {
                project_ids.insert(item.project_id.clone());
            }
        }
    }
    project_ids.into_iter().collect()
}

pub(crate) fn aggregate_task_ids_from_sources(candidates: &[AgentMemorySourceItem]) -> Vec<String> {
    let mut task_ids = BTreeSet::new();
    for item in candidates {
        if let AgentMemorySourceItem::TaskResultBrief(item) = item {
            task_ids.insert(item.task_id.clone());
        }
    }
    task_ids.into_iter().collect()
}

pub(crate) fn resolve_source_kind(candidates: &[AgentMemorySourceItem]) -> Option<String> {
    let mut kinds = BTreeSet::new();
    for item in candidates {
        match item {
            AgentMemorySourceItem::ChatSummary(_) => {
                kinds.insert("chat_summary");
            }
            AgentMemorySourceItem::TaskResultBrief(_) => {
                kinds.insert("task_result");
            }
        }
    }
    match kinds.len() {
        0 => None,
        1 => kinds.iter().next().map(|value| (*value).to_string()),
        _ => Some("mixed".to_string()),
    }
}

pub(crate) async fn select_rollup_batch(
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

pub(crate) async fn finish_failed_job_run(pool: &Db, job_run_id: &str, error_message: &str) {
    job_support::finish_failed_job_run(pool, job_run_id, error_message, "[MEMORY-AGENT-RECALL]")
        .await;
}

pub(crate) async fn resolve_model_config(
    pool: &Db,
    user_id: &str,
    model_config_id: Option<&str>,
) -> Result<Option<crate::models::AiModelConfig>, String> {
    job_support::resolve_model_config(pool, user_id, model_config_id).await
}
