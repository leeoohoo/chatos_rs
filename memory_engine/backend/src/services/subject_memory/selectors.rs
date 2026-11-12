// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::db::Db;
use crate::repositories::subject_memories;
use crate::services::ai_pipeline::estimate_tokens_text;

use super::render::{subject_memory_to_rollup_block, summary_to_subject_memory_block};
use super::{PendingSourceSummary, RollupSelection};

pub(crate) fn select_summary_batch(
    candidates: &[PendingSourceSummary],
    token_limit: i64,
) -> Option<Vec<PendingSourceSummary>> {
    if candidates.is_empty() {
        return None;
    }
    let token_sizes = candidates
        .iter()
        .map(summary_to_subject_memory_block)
        .map(|text| estimate_tokens_text(text.as_str()))
        .collect::<Vec<_>>();
    select_bounded_batch(candidates, token_sizes.as_slice(), token_limit, 1)
}

pub(crate) async fn select_rollup_batch(
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    subject_id: &str,
    relation_subject_id: &str,
    memory_type: &str,
    token_limit: i64,
) -> Result<Option<RollupSelection>, String> {
    for level in subject_memories::list_pending_subject_memory_levels(
        db,
        tenant_id,
        source_id,
        subject_id,
        relation_subject_id,
        memory_type,
    )
    .await?
    {
        let candidates = subject_memories::list_pending_subject_memories_by_level(
            db,
            tenant_id,
            source_id,
            subject_id,
            relation_subject_id,
            memory_type,
            level,
        )
        .await?;

        if candidates.len() < 2 {
            continue;
        }
        let token_sizes = candidates
            .iter()
            .map(subject_memory_to_rollup_block)
            .map(|text| estimate_tokens_text(text.as_str()))
            .collect::<Vec<_>>();
        if let Some(selected) = select_bounded_batch(
            candidates.as_slice(),
            token_sizes.as_slice(),
            token_limit,
            2,
        ) {
            return Ok(Some(RollupSelection { level, selected }));
        }
    }

    Ok(None)
}

fn select_bounded_batch<T: Clone>(
    candidates: &[T],
    token_sizes: &[i64],
    token_limit: i64,
    minimum_sources: usize,
) -> Option<Vec<T>> {
    if candidates.is_empty() || candidates.len() != token_sizes.len() {
        return None;
    }
    if token_sizes.iter().sum::<i64>() < token_limit {
        return None;
    }
    let mut selected = Vec::new();
    let mut selected_tokens = 0_i64;
    for (candidate, candidate_tokens) in candidates.iter().zip(token_sizes.iter().copied()) {
        if selected.len() >= minimum_sources
            && selected_tokens.saturating_add(candidate_tokens) > token_limit
        {
            break;
        }
        selected_tokens = selected_tokens.saturating_add(candidate_tokens);
        selected.push(candidate.clone());
    }
    Some(selected)
}

#[cfg(test)]
mod tests {
    use crate::models::EngineSummary;

    use super::{select_bounded_batch, select_summary_batch, PendingSourceSummary};

    fn pending_source_summary(idx: usize) -> PendingSourceSummary {
        let summary = EngineSummary {
            id: format!("sum-{idx}"),
            tenant_id: "tenant".to_string(),
            source_id: "source".to_string(),
            thread_id: "thread".to_string(),
            subject_id: "subject".to_string(),
            summary_type: "thread_incremental".to_string(),
            level: 0,
            source_digest: None,
            summary_text: format!("summary {idx}"),
            source_record_start_id: None,
            source_record_end_id: None,
            source_record_count: 1,
            status: "done".to_string(),
            rollup_status: "pending".to_string(),
            rollup_summary_id: None,
            rolled_up_at: None,
            subject_memory_summarized: 0,
            subject_memory_summarized_at: None,
            metadata: None,
            payload_schema: None,
            structured_payload: None,
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
        };

        PendingSourceSummary {
            id: summary.id,
            tenant_id: summary.tenant_id,
            source_id: summary.source_id,
            thread_id: summary.thread_id,
            summary_type: summary.summary_type,
            level: summary.level,
            summary_text: summary.summary_text,
            structured_payload: summary.structured_payload,
            created_at: summary.created_at,
            metadata: summary.metadata,
        }
    }

    #[test]
    fn terminal_task_summary_waits_for_subject_memory_policy_threshold() {
        let mut terminal = pending_source_summary(1);
        terminal.metadata = Some(serde_json::json!({
            "terminal_memory_merge_requested": true,
        }));

        assert!(select_summary_batch(&[terminal], 170_000).is_none());
    }

    #[test]
    fn terminal_task_summary_does_not_sweep_older_pending_summary_below_threshold() {
        let ordinary = pending_source_summary(0);
        let mut terminal = pending_source_summary(1);
        terminal.metadata = Some(serde_json::json!({
            "terminal_memory_merge_requested": true,
        }));

        assert!(select_summary_batch(&[ordinary, terminal], 170_000).is_none());
    }

    #[test]
    fn ordinary_summary_still_waits_for_subject_memory_policy_threshold() {
        assert!(select_summary_batch(&[pending_source_summary(1)], 170_000).is_none());
    }

    #[test]
    fn token_limit_caps_subject_memory_batch_instead_of_only_triggering_it() {
        let candidates = vec!["one", "two", "three", "four"];
        let selected = select_bounded_batch(&candidates, &[1_000, 1_000, 1_000, 1_000], 1_500, 1)
            .expect("token trigger");
        assert_eq!(selected, ["one"]);
    }
}
