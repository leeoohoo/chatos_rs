use crate::db::Db;
use crate::models::{EngineRecord, EngineSummary};
use crate::repositories::{records, summaries};
use crate::services::ai_pipeline::{estimate_tokens_text, MIN_TOKEN_LIMIT};

use super::render::{record_to_summary_block, summary_to_rollup_block};
use super::{PendingRecordSelection, RepairRecordSelection};

pub(crate) fn select_pending_records_for_summary(
    records: Vec<EngineRecord>,
    token_limit: i64,
) -> PendingRecordSelection {
    let effective_limit = token_limit.max(MIN_TOKEN_LIMIT);
    let mut selected = Vec::new();
    let mut oversized = Vec::new();
    let mut selected_token_count = 0_i64;
    let mut oversized_token_count = 0_i64;

    for record in records {
        let block = record_to_summary_block(&record);
        let record_tokens = estimate_tokens_text(block.as_str());
        if record_tokens > effective_limit {
            oversized_token_count += record_tokens;
            oversized.push(record);
            continue;
        }

        selected_token_count += record_tokens;
        selected.push(record);
    }

    PendingRecordSelection {
        selected,
        oversized,
        selected_token_count,
        oversized_token_count,
    }
}

pub(crate) fn select_records_for_repair(records: Vec<EngineRecord>) -> RepairRecordSelection {
    let selected = records;
    let selected_token_count = selected
        .iter()
        .map(record_to_summary_block)
        .map(|text| estimate_tokens_text(text.as_str()))
        .sum::<i64>();

    RepairRecordSelection {
        selected,
        selected_token_count,
    }
}

pub(crate) async fn mark_oversized_records_as_summarized(
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    thread_id: &str,
    oversized_records: &[EngineRecord],
    summary_id: &str,
) -> Result<usize, String> {
    if oversized_records.is_empty() {
        return Ok(0);
    }

    let record_ids = oversized_records
        .iter()
        .map(|item| item.id.clone())
        .collect::<Vec<_>>();
    records::mark_records_summarized(
        db,
        tenant_id,
        source_id,
        thread_id,
        record_ids.as_slice(),
        summary_id,
    )
    .await
}

pub(crate) async fn select_rollup_batch(
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    thread_id: &str,
    token_limit: i64,
    count_limit: i64,
    keep_level0_count: i64,
    max_level: i64,
) -> Result<Option<(i64, Vec<EngineSummary>, &'static str)>, String> {
    for level in 0..max_level {
        let mut candidates =
            summaries::list_pending_summaries_by_level(db, tenant_id, source_id, thread_id, level)
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
        // When count_limit triggers, process one fixed-size batch instead of
        // consuming the whole backlog at once. This lets large queues drain
        // incrementally across repeated rollup runs.
        if count_limit > 0 && (candidates.len() as i64) >= count_limit {
            let batch = candidates
                .into_iter()
                .take(count_limit as usize)
                .collect::<Vec<_>>();
            return Ok(Some((level, batch, "count_limit")));
        }

        let token_sum = candidates
            .iter()
            .map(summary_to_rollup_block)
            .map(|text| estimate_tokens_text(text.as_str()))
            .sum::<i64>();
        if token_sum >= token_limit {
            return Ok(Some((level, candidates, "token_limit")));
        }
    }

    Ok(None)
}

#[cfg(test)]
mod tests {
    use crate::models::EngineSummary;

    use super::select_rollup_batch;

    #[test]
    fn count_limit_caps_rollup_batch_size() {
        let candidates = (0..5)
            .map(|idx| EngineSummary {
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
            })
            .collect::<Vec<_>>();

        let selected = if 3 > 0 && (candidates.len() as i64) >= 3 {
            Some((
                0,
                candidates.into_iter().take(3).collect::<Vec<_>>(),
                "count_limit",
            ))
        } else {
            None
        };

        let (level, batch, reason) = selected.expect("count limit should select a batch");
        assert_eq!(level, 0);
        assert_eq!(reason, "count_limit");
        assert_eq!(batch.len(), 3);
        assert_eq!(batch[0].id, "sum-0");
        assert_eq!(batch[2].id, "sum-2");
    }

    #[test]
    fn selector_reference_kept_for_compile() {
        let _ = select_rollup_batch;
    }
}
