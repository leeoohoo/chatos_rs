use super::job_support;
use crate::db::Db;
use crate::models::AgentRecall;
use crate::repositories::memories;
use crate::repositories::summaries::AgentMemorySummarySource;
use crate::services::summarizer::estimate_tokens_text;

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

pub(crate) fn select_summary_batch(
    candidates: &[AgentMemorySummarySource],
    round_limit: i64,
    token_limit: i64,
) -> Option<Vec<AgentMemorySummarySource>> {
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
        .map(|item| estimate_tokens_text(item.summary_text.as_str()))
        .sum::<i64>();
    if token_sum >= token_limit {
        return Some(candidates.to_vec());
    }

    None
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
