// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeMap;

use crate::db::Db;
use crate::repositories::{subject_memories, summaries};
use crate::services::ai_pipeline::estimate_tokens_text;

use super::render::{subject_memory_to_rollup_block, summary_to_subject_memory_block};
use super::{PendingSourceSummary, RollupSelection};

pub(crate) fn select_summary_batch(
    candidates: &[PendingSourceSummary],
    token_limit: i64,
    count_limit: i64,
) -> Option<Vec<PendingSourceSummary>> {
    if candidates.is_empty() {
        return None;
    }
    // Count-based triggering should also cap the batch size so one hot scope
    // does not consume every pending summary in a single run.
    if count_limit > 0 && (candidates.len() as i64) >= count_limit {
        return Some(
            candidates
                .iter()
                .take(count_limit as usize)
                .cloned()
                .collect::<Vec<_>>(),
        );
    }

    let token_sum = candidates
        .iter()
        .map(summary_to_subject_memory_block)
        .map(|text| estimate_tokens_text(text.as_str()))
        .sum::<i64>();
    if token_sum >= token_limit {
        return Some(candidates.to_vec());
    }

    None
}

pub(crate) async fn select_rollup_batch(
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    subject_id: &str,
    relation_subject_id: &str,
    memory_type: &str,
    token_limit: i64,
    count_limit: i64,
    keep_level0_count: i64,
    max_level: i64,
) -> Result<Option<RollupSelection>, String> {
    for level in 0..max_level {
        let mut candidates = subject_memories::list_pending_subject_memories_by_level(
            db,
            tenant_id,
            source_id,
            subject_id,
            relation_subject_id,
            memory_type,
            level,
        )
        .await?;

        if level == 0 && keep_level0_count > 0 {
            let keep = keep_level0_count as usize;
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
        if count_limit > 0 && (candidates.len() as i64) >= count_limit {
            let batch = candidates
                .into_iter()
                .take(count_limit as usize)
                .collect::<Vec<_>>();
            return Ok(Some(RollupSelection {
                level,
                selected: batch,
            }));
        }

        let token_sum = candidates
            .iter()
            .map(subject_memory_to_rollup_block)
            .map(|text| estimate_tokens_text(text.as_str()))
            .sum::<i64>();
        if token_sum >= token_limit {
            return Ok(Some(RollupSelection {
                level,
                selected: candidates,
            }));
        }
    }

    Ok(None)
}

pub(crate) async fn mark_summary_sources_subject_memory_summarized(
    db: &Db,
    selected: &[PendingSourceSummary],
) -> Result<usize, String> {
    if selected.is_empty() {
        return Ok(0);
    }

    let mut grouped = BTreeMap::<(String, String, String), Vec<String>>::new();
    for item in selected {
        grouped
            .entry((
                item.tenant_id.clone(),
                item.source_id.clone(),
                item.thread_id.clone(),
            ))
            .or_default()
            .push(item.id.clone());
    }

    let mut marked = 0usize;
    for ((tenant_id, source_id, thread_id), summary_ids) in grouped {
        marked += summaries::mark_summaries_subject_memory_summarized(
            db,
            tenant_id.as_str(),
            source_id.as_str(),
            thread_id.as_str(),
            summary_ids.as_slice(),
        )
        .await?;
    }

    Ok(marked)
}

#[cfg(test)]
mod tests {
    use crate::models::EngineSummary;

    use super::{select_summary_batch, PendingSourceSummary};

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
            created_at: summary.created_at,
            metadata: summary.metadata,
        }
    }

    #[test]
    fn count_limit_caps_subject_memory_level0_batch_size() {
        let candidates = (0..5).map(pending_source_summary).collect::<Vec<_>>();

        let selected =
            select_summary_batch(candidates.as_slice(), 50_000, 3).expect("batch should exist");

        assert_eq!(selected.len(), 3);
        assert_eq!(selected[0].id, "sum-0");
        assert_eq!(selected[2].id, "sum-2");
    }

    #[test]
    fn summary_batch_preserves_nonzero_summary_levels() {
        let mut level1 = pending_source_summary(1);
        level1.level = 1;
        let candidates = vec![pending_source_summary(0), level1];

        let selected =
            select_summary_batch(candidates.as_slice(), 50_000, 2).expect("batch should exist");

        assert_eq!(selected.len(), 2);
        assert_eq!(selected[0].level, 0);
        assert_eq!(selected[1].level, 1);
    }
}
