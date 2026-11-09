// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::db::Db;
use crate::models::{EngineRecord, EngineSummary};
use crate::repositories::summaries;
use crate::services::ai_pipeline::{estimate_tokens_text, MIN_TOKEN_LIMIT};

use super::render::{record_to_summary_block, summary_to_rollup_block};
use super::PendingRecordSelection;

pub(crate) fn select_pending_records_for_summary(
    records: Vec<EngineRecord>,
    token_limit: i64,
) -> PendingRecordSelection {
    let effective_limit = token_limit.max(MIN_TOKEN_LIMIT);
    let mut selected = Vec::new();
    let oversized = Vec::new();
    let mut selected_token_count = 0_i64;
    let oversized_token_count = 0_i64;
    let project_subject_id = records.first().and_then(record_project_subject_id);

    for record in records {
        if record_project_subject_id(&record) != project_subject_id {
            break;
        }
        let block = record_to_summary_block(&record);
        let record_tokens = estimate_tokens_text(block.as_str());
        if record_tokens > effective_limit {
            if selected.is_empty() {
                selected_token_count = record_tokens;
                selected.push(record);
            }
            break;
        }

        if !selected.is_empty()
            && selected_token_count.saturating_add(record_tokens) > effective_limit
        {
            break;
        }

        selected_token_count += record_tokens;
        selected.push(record);
    }

    PendingRecordSelection {
        selected,
        oversized,
        selected_token_count,
        oversized_token_count,
        project_subject_id,
    }
}

fn record_project_subject_id(record: &EngineRecord) -> Option<String> {
    let project_id = record
        .metadata
        .as_ref()
        .and_then(|metadata| metadata.get("memory_project_id"))
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())?;
    Some(if project_id.starts_with("project:") {
        project_id.to_string()
    } else {
        format!("project:{project_id}")
    })
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
) -> Result<Option<(i64, String, Vec<EngineSummary>, &'static str)>, String> {
    for level in 0..max_level {
        let partition_keys = summaries::list_pending_summary_partition_keys_by_level(
            db, tenant_id, source_id, thread_id, level,
        )
        .await?;
        for project_partition_key in partition_keys {
            let mut candidates = summaries::list_pending_summaries_by_level_and_partition(
                db,
                tenant_id,
                source_id,
                thread_id,
                project_partition_key.as_str(),
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
            if let Some((selected, reason)) =
                select_bounded_rollup_candidates(candidates, token_limit, count_limit)
            {
                return Ok(Some((level, project_partition_key, selected, reason)));
            }
        }
    }

    Ok(None)
}

fn select_bounded_rollup_candidates(
    candidates: Vec<EngineSummary>,
    token_limit: i64,
    count_limit: i64,
) -> Option<(Vec<EngineSummary>, &'static str)> {
    if candidates.is_empty() {
        return None;
    }
    let token_limit = token_limit.max(MIN_TOKEN_LIMIT);
    let token_sizes = candidates
        .iter()
        .map(summary_to_rollup_block)
        .map(|text| estimate_tokens_text(text.as_str()))
        .collect::<Vec<_>>();
    let count_triggered = count_limit > 0 && (candidates.len() as i64) >= count_limit;
    if !count_triggered && token_sizes.iter().sum::<i64>() < token_limit {
        return None;
    }

    let candidate_limit = if count_triggered {
        count_limit as usize
    } else {
        candidates.len()
    };
    let mut selected = Vec::new();
    let mut selected_tokens = 0_i64;
    for (candidate, candidate_tokens) in candidates
        .into_iter()
        .zip(token_sizes)
        .take(candidate_limit)
    {
        if !selected.is_empty() && selected_tokens.saturating_add(candidate_tokens) > token_limit {
            break;
        }
        selected_tokens = selected_tokens.saturating_add(candidate_tokens);
        selected.push(candidate);
    }
    Some((
        selected,
        if count_triggered {
            "count_limit"
        } else {
            "token_limit"
        },
    ))
}

#[cfg(test)]
mod tests {
    use crate::models::{EngineRecord, EngineSummary};

    use super::{
        select_bounded_rollup_candidates, select_pending_records_for_summary, select_rollup_batch,
    };

    fn record(id: &str, content: &str) -> EngineRecord {
        EngineRecord {
            id: id.to_string(),
            thread_id: "thread".to_string(),
            tenant_id: "tenant".to_string(),
            source_id: "source".to_string(),
            external_record_id: None,
            role: "tool".to_string(),
            record_type: "message".to_string(),
            content: content.to_string(),
            structured_payload: None,
            metadata: None,
            summary_status: "pending".to_string(),
            summary_id: None,
            summarized_at: None,
            created_at: "2026-01-01T00:00:00Z".to_string(),
        }
    }

    fn project_record(id: &str, project_id: &str) -> EngineRecord {
        let mut value = record(id, "project fact");
        value.metadata = Some(serde_json::json!({"memory_project_id": project_id}));
        value
    }

    #[test]
    fn pending_selector_caps_the_cumulative_batch_instead_of_only_each_record() {
        let content = "x".repeat(2_000);
        let selection = select_pending_records_for_summary(
            vec![
                record("record-1", content.as_str()),
                record("record-2", content.as_str()),
                record("record-3", content.as_str()),
            ],
            1_100,
        );

        assert_eq!(selection.selected.len(), 2);
        assert!(selection.selected_token_count <= 1_100);
        assert!(selection.oversized.is_empty());
    }

    #[test]
    fn pending_selector_never_mixes_project_segments() {
        let selection = select_pending_records_for_summary(
            vec![
                project_record("project-a-1", "a"),
                project_record("project-a-2", "a"),
                project_record("project-b-1", "b"),
            ],
            10_000,
        );

        assert_eq!(selection.project_subject_id.as_deref(), Some("project:a"));
        assert_eq!(selection.selected.len(), 2);
        assert_eq!(selection.selected[0].id, "project-a-1");
        assert_eq!(selection.selected[1].id, "project-a-2");
    }

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
                payload_schema: None,
                structured_payload: None,
                created_at: "2026-01-01T00:00:00Z".to_string(),
                updated_at: "2026-01-01T00:00:00Z".to_string(),
            })
            .collect::<Vec<_>>();

        let (batch, reason) =
            select_bounded_rollup_candidates(candidates, 50_000, 3).expect("count trigger");
        assert_eq!(reason, "count_limit");
        assert_eq!(batch.len(), 3);
        assert_eq!(batch[0].id, "sum-0");
        assert_eq!(batch[2].id, "sum-2");
    }

    #[test]
    fn token_limit_caps_rollup_batch_instead_of_only_triggering_it() {
        let candidates = (0..4)
            .map(|idx| EngineSummary {
                id: format!("sum-{idx}"),
                tenant_id: "tenant".to_string(),
                source_id: "source".to_string(),
                thread_id: "thread".to_string(),
                subject_id: "subject".to_string(),
                summary_type: "thread_incremental".to_string(),
                level: 0,
                source_digest: None,
                summary_text: "x".repeat(4_000),
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
            })
            .collect::<Vec<_>>();

        let (selected, reason) =
            select_bounded_rollup_candidates(candidates, 1_500, 0).expect("token trigger");
        assert_eq!(reason, "token_limit");
        assert_eq!(selected.len(), 1);
    }

    #[test]
    fn selector_reference_kept_for_compile() {
        let _ = select_rollup_batch;
    }
}
