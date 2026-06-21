use std::collections::HashSet;

use mongodb::{
    bson::{Bson, doc},
    options::ReturnDocument,
};
use tokio::task::JoinSet;

use crate::db::Db;
use crate::models::{BatchSyncRecordsRequest, EngineRecord, UpsertRecordInput};
use crate::repositories::threads;

use super::common::{
    estimate_pending_record_tokens, estimate_record_summary_tokens, record_collection,
    summary_status_is_pending,
};
use super::compact_turns;

const BATCH_SYNC_CONCURRENCY: usize = 32;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
struct SummaryQueueDelta {
    pending_record_count_delta: i64,
    pending_summary_tokens_delta: i64,
}

impl SummaryQueueDelta {
    fn merge(&mut self, other: Self) {
        self.pending_record_count_delta += other.pending_record_count_delta;
        self.pending_summary_tokens_delta += other.pending_summary_tokens_delta;
    }

    fn is_zero(self) -> bool {
        self.pending_record_count_delta == 0 && self.pending_summary_tokens_delta == 0
    }
}

#[derive(Debug, Default)]
struct BatchSyncOutcome {
    upserted_count: usize,
    summary_queue_delta: SummaryQueueDelta,
    compact_turn_keys: Vec<CompactTurnKey>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct CompactTurnKey {
    thread_id: String,
    tenant_id: String,
    source_id: String,
    record_type: String,
    turn_id: String,
}

impl CompactTurnKey {
    fn new(
        thread_id: &str,
        tenant_id: &str,
        source_id: &str,
        record_type: &str,
        metadata: Option<&serde_json::Value>,
    ) -> Option<Self> {
        compact_turns::extract_turn_id_from_metadata(metadata).map(|turn_id| Self {
            thread_id: thread_id.to_string(),
            tenant_id: tenant_id.to_string(),
            source_id: source_id.to_string(),
            record_type: record_type.to_string(),
            turn_id: turn_id.to_string(),
        })
    }

    fn from_record(record: &EngineRecord) -> Option<Self> {
        Self::new(
            record.thread_id.as_str(),
            record.tenant_id.as_str(),
            record.source_id.as_str(),
            record.record_type.as_str(),
            record.metadata.as_ref(),
        )
    }
}

pub async fn batch_sync_records(
    db: &Db,
    thread_id: &str,
    req: &BatchSyncRecordsRequest,
) -> Result<usize, String> {
    let collection = record_collection(db);
    let mut upserted_count = 0usize;
    let mut summary_queue_delta = SummaryQueueDelta::default();
    let mut compact_turn_keys = Vec::new();
    let mut join_set = JoinSet::new();

    for record in &req.records {
        let collection = collection.clone();
        let thread_id = thread_id.to_string();
        let tenant_id = req.tenant_id.clone();
        let source_id = req.source_id.clone();
        let record = record.clone();
        join_set.spawn(async move {
            sync_one_record(collection, thread_id, tenant_id, source_id, record).await
        });

        if join_set.len() >= BATCH_SYNC_CONCURRENCY {
            let outcome = consume_one_upsert_result(&mut join_set).await?;
            upserted_count += outcome.upserted_count;
            summary_queue_delta.merge(outcome.summary_queue_delta);
            compact_turn_keys.extend(outcome.compact_turn_keys);
        }
    }

    while !join_set.is_empty() {
        let outcome = consume_one_upsert_result(&mut join_set).await?;
        upserted_count += outcome.upserted_count;
        summary_queue_delta.merge(outcome.summary_queue_delta);
        compact_turn_keys.extend(outcome.compact_turn_keys);
    }

    rebuild_compact_turns(db, compact_turn_keys).await?;

    if !summary_queue_delta.is_zero() {
        let _ = threads::apply_summary_queue_state_delta(
            db,
            req.tenant_id.as_str(),
            req.source_id.as_str(),
            thread_id,
            summary_queue_delta.pending_record_count_delta,
            summary_queue_delta.pending_summary_tokens_delta,
        )
        .await;
    }

    Ok(upserted_count)
}

async fn sync_one_record(
    collection: mongodb::Collection<EngineRecord>,
    thread_id: String,
    tenant_id: String,
    source_id: String,
    record: UpsertRecordInput,
) -> Result<BatchSyncOutcome, String> {
    let summary_status = record
        .summary_status
        .clone()
        .unwrap_or_else(|| "pending".to_string());
    let previous = collection
        .find_one_and_update(
            doc! {
                "tenant_id": &tenant_id,
                "source_id": &source_id,
                "thread_id": &thread_id,
                "id": &record.id,
            },
            doc! {
                "$set": {
                    "thread_id": &thread_id,
                    "tenant_id": &tenant_id,
                    "source_id": &source_id,
                    "external_record_id": mongodb::bson::to_bson(&record.external_record_id).unwrap_or(Bson::Null),
                    "role": &record.role,
                    "record_type": &record.record_type,
                    "content": &record.content,
                    "structured_payload": mongodb::bson::to_bson(&record.structured_payload).unwrap_or(Bson::Null),
                    "metadata": mongodb::bson::to_bson(&record.metadata).unwrap_or(Bson::Null),
                    "summary_status": &summary_status,
                    "summary_id": mongodb::bson::to_bson(&record.summary_id).unwrap_or(Bson::Null),
                    "summarized_at": mongodb::bson::to_bson(&record.summarized_at).unwrap_or(Bson::Null),
                    "created_at": &record.created_at,
                }
            },
        )
        .upsert(true)
        .return_document(ReturnDocument::Before)
        .await
        .map_err(|err| err.to_string())?;

    let mut compact_turn_keys = Vec::new();
    if let Some(key) = CompactTurnKey::new(
        thread_id.as_str(),
        tenant_id.as_str(),
        source_id.as_str(),
        record.record_type.as_str(),
        record.metadata.as_ref(),
    ) {
        compact_turn_keys.push(key);
    }
    if let Some(key) = previous.as_ref().and_then(CompactTurnKey::from_record) {
        compact_turn_keys.push(key);
    }

    Ok(BatchSyncOutcome {
        upserted_count: usize::from(previous.is_none()),
        summary_queue_delta: calculate_summary_queue_delta(
            previous.as_ref(),
            &record,
            summary_status.as_str(),
        ),
        compact_turn_keys,
    })
}

async fn rebuild_compact_turns(db: &Db, keys: Vec<CompactTurnKey>) -> Result<(), String> {
    let mut seen = HashSet::new();
    for key in keys {
        if !seen.insert(key.clone()) {
            continue;
        }
        compact_turns::rebuild_compact_turn(
            db,
            key.thread_id.as_str(),
            key.tenant_id.as_str(),
            key.source_id.as_str(),
            key.record_type.as_str(),
            key.turn_id.as_str(),
        )
        .await?;
    }
    Ok(())
}

fn calculate_summary_queue_delta(
    previous: Option<&EngineRecord>,
    record: &UpsertRecordInput,
    summary_status: &str,
) -> SummaryQueueDelta {
    let previous_pending = previous
        .map(|item| summary_status_is_pending(Some(item.summary_status.as_str())))
        .unwrap_or(false);
    let next_pending = summary_status_is_pending(Some(summary_status));

    let previous_tokens = previous
        .filter(|_| previous_pending)
        .map(estimate_pending_record_tokens)
        .unwrap_or(0);
    let next_tokens = if next_pending {
        estimate_record_summary_tokens(
            record.created_at.as_str(),
            record.role.as_str(),
            record.content.as_str(),
            record.structured_payload.as_ref(),
            record.metadata.as_ref(),
        )
    } else {
        0
    };

    SummaryQueueDelta {
        pending_record_count_delta: i64::from(next_pending) - i64::from(previous_pending),
        pending_summary_tokens_delta: next_tokens - previous_tokens,
    }
}

async fn consume_one_upsert_result(
    join_set: &mut JoinSet<Result<BatchSyncOutcome, String>>,
) -> Result<BatchSyncOutcome, String> {
    let next = join_set
        .join_next()
        .await
        .ok_or_else(|| "batch sync worker exited unexpectedly".to_string())?;
    next.map_err(|err| err.to_string())?
}

pub async fn delete_records_by_thread(
    db: &Db,
    thread_id: &str,
    tenant_id: &str,
    source_id: &str,
    record_type: Option<&str>,
) -> Result<i64, String> {
    let mut filter = doc! {
        "tenant_id": tenant_id,
        "source_id": source_id,
        "thread_id": thread_id,
    };
    if let Some(value) = record_type.map(str::trim).filter(|value| !value.is_empty()) {
        filter.insert("record_type", value);
    }

    let result = record_collection(db)
        .delete_many(filter)
        .await
        .map_err(|err| err.to_string())?;
    compact_turns::delete_compact_turns_by_thread(db, thread_id, tenant_id, source_id, record_type)
        .await?;
    Ok(result.deleted_count as i64)
}

pub async fn delete_record_by_id(
    db: &Db,
    record_id: &str,
    tenant_id: &str,
    source_id: &str,
    thread_id: Option<&str>,
) -> Result<bool, String> {
    let mut filter = doc! {
        "tenant_id": tenant_id,
        "source_id": source_id,
        "id": record_id,
    };
    if let Some(value) = thread_id.map(str::trim).filter(|value| !value.is_empty()) {
        filter.insert("thread_id", value);
    }

    let deleted = record_collection(db)
        .find_one_and_delete(filter)
        .await
        .map_err(|err| err.to_string())?;
    if let Some(record) = deleted.as_ref() {
        compact_turns::rebuild_compact_turn_for_record(
            db,
            record.thread_id.as_str(),
            record.tenant_id.as_str(),
            record.source_id.as_str(),
            record.record_type.as_str(),
            record.metadata.as_ref(),
        )
        .await?;
    }
    Ok(deleted.is_some())
}

#[cfg(test)]
mod tests {
    use super::{SummaryQueueDelta, calculate_summary_queue_delta};
    use crate::models::{EngineRecord, UpsertRecordInput};
    use crate::repositories::records::common::estimate_record_summary_tokens;

    #[test]
    fn calculate_delta_counts_new_pending_record() {
        let record = upsert_record("r-1", "hello", None);

        let delta = calculate_summary_queue_delta(None, &record, "pending");

        assert_eq!(
            delta,
            SummaryQueueDelta {
                pending_record_count_delta: 1,
                pending_summary_tokens_delta: estimate_record_summary_tokens(
                    record.created_at.as_str(),
                    record.role.as_str(),
                    record.content.as_str(),
                    record.structured_payload.as_ref(),
                    record.metadata.as_ref(),
                ),
            }
        );
    }

    #[test]
    fn calculate_delta_removes_pending_record_when_summarized() {
        let previous = engine_record("r-1", "hello world", "pending");
        let next = upsert_record("r-1", "hello world", Some("summarized"));

        let delta = calculate_summary_queue_delta(Some(&previous), &next, "summarized");

        assert_eq!(
            delta,
            SummaryQueueDelta {
                pending_record_count_delta: -1,
                pending_summary_tokens_delta: -estimate_record_summary_tokens(
                    previous.created_at.as_str(),
                    previous.role.as_str(),
                    previous.content.as_str(),
                    previous.structured_payload.as_ref(),
                    previous.metadata.as_ref(),
                ),
            }
        );
    }

    #[test]
    fn calculate_delta_updates_tokens_when_pending_content_changes() {
        let previous = engine_record("r-1", "short", "pending");
        let next = upsert_record("r-1", "a much longer pending record", Some("pending"));

        let delta = calculate_summary_queue_delta(Some(&previous), &next, "pending");

        assert_eq!(delta.pending_record_count_delta, 0);
        assert_eq!(
            delta.pending_summary_tokens_delta,
            estimate_record_summary_tokens(
                next.created_at.as_str(),
                next.role.as_str(),
                next.content.as_str(),
                next.structured_payload.as_ref(),
                next.metadata.as_ref(),
            ) - estimate_record_summary_tokens(
                previous.created_at.as_str(),
                previous.role.as_str(),
                previous.content.as_str(),
                previous.structured_payload.as_ref(),
                previous.metadata.as_ref(),
            )
        );
    }

    fn upsert_record(id: &str, content: &str, summary_status: Option<&str>) -> UpsertRecordInput {
        UpsertRecordInput {
            id: id.to_string(),
            external_record_id: None,
            role: "user".to_string(),
            record_type: "message".to_string(),
            content: content.to_string(),
            structured_payload: None,
            metadata: None,
            summary_status: summary_status.map(str::to_string),
            summary_id: None,
            summarized_at: None,
            created_at: "2026-05-20T12:00:00Z".to_string(),
        }
    }

    fn engine_record(id: &str, content: &str, summary_status: &str) -> EngineRecord {
        EngineRecord {
            id: id.to_string(),
            thread_id: "thread-1".to_string(),
            tenant_id: "tenant-1".to_string(),
            source_id: "source-1".to_string(),
            external_record_id: None,
            role: "user".to_string(),
            record_type: "message".to_string(),
            content: content.to_string(),
            structured_payload: None,
            metadata: None,
            summary_status: summary_status.to_string(),
            summary_id: None,
            summarized_at: None,
            created_at: "2026-05-20T12:00:00Z".to_string(),
        }
    }
}
