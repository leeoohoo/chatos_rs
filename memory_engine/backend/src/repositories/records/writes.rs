// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::{HashMap, HashSet};

use sqlx::types::Json;

use crate::config::AppConfig;
use crate::db::Db;
use crate::models::{
    BatchSyncRecordsRequest, EngineRecord, UpsertRecordInput, UpsertSubjectRequest,
};
use crate::repositories::postgres::{decode, json, timestamp};
use crate::repositories::{subject_memory_scopes, subjects, threads};

use super::common::{
    estimate_pending_record_tokens, estimate_record_summary_tokens, summary_status_is_pending,
};
use super::compact_turns;

#[derive(Debug, Default, Clone, Copy)]
struct SummaryQueueDelta {
    count: i64,
    tokens: i64,
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
    fn from_record(record: &EngineRecord) -> Option<Self> {
        compact_turns::extract_turn_id_from_metadata(record.metadata.as_ref()).map(|turn_id| Self {
            thread_id: record.thread_id.clone(),
            tenant_id: record.tenant_id.clone(),
            source_id: record.source_id.clone(),
            record_type: record.record_type.clone(),
            turn_id: turn_id.to_string(),
        })
    }
}

pub async fn batch_sync_records(
    config: &AppConfig,
    db: &Db,
    thread_id: &str,
    req: &BatchSyncRecordsRequest,
) -> Result<usize, String> {
    batch_sync_records_with_partition_dispatch(config, db, thread_id, req, false).await
}

pub async fn batch_sync_records_with_partition_dispatch(
    config: &AppConfig,
    db: &Db,
    thread_id: &str,
    req: &BatchSyncRecordsRequest,
    partition_dispatch_v2: bool,
) -> Result<usize, String> {
    threads::begin_record_sync(
        db,
        &req.tenant_id,
        &req.source_id,
        thread_id,
        config.record_sync_lease_timeout_secs,
    )
    .await?;
    let result = batch_sync_records_inner(db, thread_id, req, partition_dispatch_v2).await;
    let finish = threads::finish_record_sync(db, &req.tenant_id, &req.source_id, thread_id).await;
    match (result, finish) {
        (Ok(value), Ok(())) => Ok(value),
        (Err(error), _) => Err(error),
        (Ok(_), Err(error)) => Err(format!("release record sync lease failed: {error}")),
    }
}

async fn batch_sync_records_inner(
    db: &Db,
    thread_id: &str,
    req: &BatchSyncRecordsRequest,
    partition_dispatch_v2: bool,
) -> Result<usize, String> {
    let project_partition_key = partition_dispatch_v2
        .then(|| batch_project_partition(req.records.as_slice()))
        .transpose()?;
    let mut tx = db.begin().await.map_err(|e| e.to_string())?;
    let mut locked_thread = if let Some(project_partition_key) = project_partition_key.as_deref() {
        let thread =
            threads::lock_for_record_ingest(&mut tx, &req.tenant_id, &req.source_id, thread_id)
                .await?;
        crate::repositories::thread_partitions::lock_or_create(
            &mut tx,
            &req.tenant_id,
            &req.source_id,
            thread_id,
            project_partition_key,
        )
        .await?;
        Some(thread)
    } else {
        None
    };
    let mut inserted = 0usize;
    let mut inserted_terminal = false;
    let mut delta = SummaryQueueDelta::default();
    let mut keys = HashSet::new();
    let mut project_identities = HashMap::<String, Option<String>>::new();
    for input in &req.records {
        let previous=sqlx::query_as::<_,(Json<serde_json::Value>,Option<String>)>(
            "SELECT data,project_partition_key FROM engine_records WHERE tenant_id=$1 AND source_id=$2 AND thread_id=$3 AND id=$4 FOR UPDATE"
        ).bind(&req.tenant_id).bind(&req.source_id).bind(thread_id).bind(&input.id)
          .fetch_optional(&mut *tx).await.map_err(|e|e.to_string())?;
        let (previous, previous_partition) = match previous {
            Some((data, partition)) => (Some(decode::<EngineRecord>(data)?), partition),
            None => (None, None),
        };
        let mut record = make_record(thread_id, &req.tenant_id, &req.source_id, input.clone());
        if let Some(previous) = previous.as_ref() {
            // A durable client may replay a committed batch after losing the HTTP response.
            // Summary lifecycle belongs to Memory Engine and must never be reset by that replay.
            record.summary_status = previous.summary_status.clone();
            record.summary_id = previous.summary_id.clone();
            record.summarized_at = previous.summarized_at.clone();
        }
        if let Some(partition) = project_partition_key.as_deref() {
            ensure_idempotent_record_replay(
                previous.as_ref(),
                previous_partition.as_deref(),
                &record,
                partition,
            )?;
        }
        if let Some((project_subject_id, project_name)) = record_project_identity(&record)? {
            let stored_name = project_identities.entry(project_subject_id).or_default();
            if project_name.is_some() {
                *stored_name = project_name;
            }
        }
        inserted += usize::from(previous.is_none());
        inserted_terminal |= previous.is_none() && terminal_summary_requested(input);
        merge_delta(&mut delta, previous.as_ref(), &record);
        if let Some(key) = CompactTurnKey::from_record(&record) {
            keys.insert(key);
        }
        if let Some(key) = previous.as_ref().and_then(CompactTurnKey::from_record) {
            keys.insert(key);
        }
        upsert_record_row_with_partition(&mut tx, &record, project_partition_key.as_deref())
            .await?;
    }
    if let (Some(project_partition_key), Some(thread)) =
        (project_partition_key.as_deref(), locked_thread.as_mut())
    {
        threads::apply_record_ingest_state(
            &mut tx,
            thread,
            delta.count,
            delta.tokens,
            inserted_terminal,
        )
        .await?;
        let partition_dispatch_version =
            crate::repositories::thread_partitions::apply_ingest_delta(
                &mut tx,
                &req.tenant_id,
                &req.source_id,
                thread_id,
                project_partition_key,
                delta.count,
                delta.tokens,
                inserted_terminal,
            )
            .await?;
        if let Some(version) = partition_dispatch_version {
            crate::repositories::derived_events::enqueue_thread_summary_request(
                &mut tx,
                &req.tenant_id,
                &req.source_id,
                thread_id,
                project_partition_key,
                version,
            )
            .await?;
        }
    }
    tx.commit().await.map_err(|e| e.to_string())?;
    for (project_subject_id, project_name) in project_identities {
        subject_memory_scopes::ensure_project_identity_scope(
            db,
            req.tenant_id.as_str(),
            req.source_id.as_str(),
            project_subject_id.as_str(),
        )
        .await?;
        if let Some(project_name) = project_name {
            subjects::upsert_subject(
                db,
                project_subject_id.as_str(),
                UpsertSubjectRequest {
                    tenant_id: req.tenant_id.clone(),
                    source_id: req.source_id.clone(),
                    subject_type: "project".to_string(),
                    display_name: Some(project_name),
                    attributes: None,
                    status: Some("active".to_string()),
                },
            )
            .await?;
        }
    }
    for key in keys {
        compact_turns::rebuild_compact_turn(
            db,
            &key.thread_id,
            &key.tenant_id,
            &key.source_id,
            &key.record_type,
            &key.turn_id,
        )
        .await?;
    }
    if !partition_dispatch_v2 && (delta.count != 0 || delta.tokens != 0) {
        threads::apply_summary_queue_state_delta(
            db,
            &req.tenant_id,
            &req.source_id,
            thread_id,
            delta.count,
            delta.tokens,
        )
        .await?;
    }
    if !partition_dispatch_v2 && req.records.iter().any(terminal_summary_requested) {
        threads::request_forced_summary_dispatch(db, &req.tenant_id, &req.source_id, thread_id)
            .await?;
    }
    Ok(inserted)
}

fn batch_project_partition(records: &[UpsertRecordInput]) -> Result<String, String> {
    let mut partition: Option<String> = None;
    for record in records {
        let current = project_partition_from_metadata(record.metadata.as_ref())?;
        if partition.as_ref().is_some_and(|value| value != &current) {
            return Err("record batch must contain exactly one project_partition_key".to_string());
        }
        partition = Some(current);
    }
    Ok(partition.unwrap_or_else(|| "__none__".to_string()))
}

fn project_partition_from_metadata(metadata: Option<&serde_json::Value>) -> Result<String, String> {
    let Some(project_id) = metadata
        .and_then(|metadata| metadata.get("memory_project_id"))
        .filter(|value| !value.is_null())
    else {
        return Ok("__none__".to_string());
    };
    let project_id = project_id
        .as_str()
        .ok_or_else(|| "record memory_project_id must be a string".to_string())?
        .trim();
    if project_id.is_empty() {
        return Ok("__none__".to_string());
    }
    let normalized = project_id.strip_prefix("project:").unwrap_or(project_id);
    if normalized.is_empty() || normalized.chars().any(char::is_control) {
        return Err("record memory_project_id is invalid".to_string());
    }
    Ok(format!("project:{normalized}"))
}

fn ensure_idempotent_record_replay(
    previous: Option<&EngineRecord>,
    previous_partition: Option<&str>,
    next: &EngineRecord,
    next_partition: &str,
) -> Result<(), String> {
    let Some(previous) = previous else {
        return Ok(());
    };
    let inferred_previous_partition = project_partition_from_metadata(previous.metadata.as_ref())?;
    if previous_partition.unwrap_or(inferred_previous_partition.as_str()) != next_partition {
        return Err("record replay cannot change project_partition_key".to_string());
    }
    if previous.external_record_id != next.external_record_id
        || previous.role != next.role
        || previous.record_type != next.record_type
        || previous.content != next.content
        || previous.structured_payload != next.structured_payload
        || previous.metadata != next.metadata
        || previous.created_at != next.created_at
    {
        return Err(
            "record replay content identity does not match the committed record".to_string(),
        );
    }
    Ok(())
}

fn terminal_summary_requested(input: &UpsertRecordInput) -> bool {
    input
        .metadata
        .as_ref()
        .and_then(|metadata| metadata.get("memory_terminal_summary_requested"))
        .and_then(serde_json::Value::as_bool)
        == Some(true)
}

fn record_project_identity(
    record: &EngineRecord,
) -> Result<Option<(String, Option<String>)>, String> {
    let Some(metadata) = record.metadata.as_ref() else {
        return Ok(None);
    };
    let Some(project_id) = metadata
        .get("memory_project_id")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return Ok(None);
    };
    let project_subject_id = if project_id.starts_with("project:") {
        project_id.to_string()
    } else {
        format!("project:{project_id}")
    };
    let project_name = match metadata.get("project_name") {
        None | Some(serde_json::Value::Null) => None,
        Some(serde_json::Value::String(value)) => {
            let value = value.trim();
            if value.is_empty()
                || value.chars().count() > 240
                || value.chars().any(char::is_control)
            {
                return Err("record project_name must be 1..=240 printable characters".to_string());
            }
            Some(value.to_string())
        }
        Some(_) => return Err("record project_name must be a string".to_string()),
    };
    Ok(Some((project_subject_id, project_name)))
}

#[cfg(test)]
mod project_identity_tests {
    use super::{record_project_identity, terminal_summary_requested};
    use crate::models::EngineRecord;
    use crate::models::UpsertRecordInput;

    fn record(metadata: serde_json::Value) -> EngineRecord {
        EngineRecord {
            id: "record-1".to_string(),
            thread_id: "thread-1".to_string(),
            tenant_id: "tenant-1".to_string(),
            source_id: "chatos".to_string(),
            external_record_id: None,
            role: "user".to_string(),
            record_type: "message".to_string(),
            content: "hello".to_string(),
            structured_payload: None,
            metadata: Some(metadata),
            summary_status: "pending".to_string(),
            summary_id: None,
            summarized_at: None,
            created_at: "2026-11-05T00:00:00Z".to_string(),
        }
    }

    #[test]
    fn project_identity_keeps_the_application_display_name() {
        let identity = record_project_identity(&record(serde_json::json!({
            "memory_project_id": "project-1",
            "project_name": " 红果逆向 ",
        })))
        .expect("valid project identity")
        .expect("project identity");

        assert_eq!(identity.0, "project:project-1");
        assert_eq!(identity.1.as_deref(), Some("红果逆向"));
    }

    #[test]
    fn project_identity_rejects_non_string_display_names() {
        assert!(record_project_identity(&record(serde_json::json!({
            "memory_project_id": "project-1",
            "project_name": 42,
        })))
        .is_err());
    }

    #[test]
    fn only_explicit_true_terminal_marker_requests_a_forced_summary() {
        let input = |metadata| UpsertRecordInput {
            id: "record-1".to_string(),
            external_record_id: None,
            role: "assistant".to_string(),
            record_type: "message".to_string(),
            content: "done".to_string(),
            structured_payload: None,
            metadata,
            summary_status: None,
            summary_id: None,
            summarized_at: None,
            created_at: "2026-11-05T00:00:00Z".to_string(),
        };

        assert!(terminal_summary_requested(&input(Some(
            serde_json::json!({
                "memory_terminal_summary_requested": true
            })
        ))));
        assert!(!terminal_summary_requested(&input(Some(
            serde_json::json!({
                "memory_terminal_summary_requested": false
            })
        ))));
        assert!(!terminal_summary_requested(&input(None)));
    }
}

pub(crate) async fn upsert_record_row(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    record: &EngineRecord,
) -> Result<(), String> {
    upsert_record_row_with_partition(tx, record, None).await
}

async fn upsert_record_row_with_partition(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    record: &EngineRecord,
    project_partition_key: Option<&str>,
) -> Result<(), String> {
    let result = sqlx::query(
        "INSERT INTO engine_records(id,thread_id,tenant_id,source_id,external_record_id,role,record_type,summary_status,summary_id,created_at,data,project_partition_key) \
         SELECT $1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12 \
         WHERE EXISTS(SELECT 1 FROM engine_threads \
         WHERE id=$2 AND tenant_id=$3 AND source_id=$4) \
         ON CONFLICT(tenant_id,source_id,id) DO UPDATE SET \
         external_record_id=EXCLUDED.external_record_id,role=EXCLUDED.role,record_type=EXCLUDED.record_type, \
         summary_status=EXCLUDED.summary_status,summary_id=EXCLUDED.summary_id,created_at=EXCLUDED.created_at,data=EXCLUDED.data, \
         project_partition_key=COALESCE(engine_records.project_partition_key,EXCLUDED.project_partition_key) \
         WHERE engine_records.thread_id=EXCLUDED.thread_id AND (EXCLUDED.project_partition_key IS NULL \
           OR engine_records.project_partition_key IS NULL \
           OR engine_records.project_partition_key=EXCLUDED.project_partition_key)",
    )
    .bind(&record.id)
    .bind(&record.thread_id)
    .bind(&record.tenant_id)
    .bind(&record.source_id)
    .bind(&record.external_record_id)
    .bind(&record.role)
    .bind(&record.record_type)
    .bind(&record.summary_status)
    .bind(&record.summary_id)
    .bind(timestamp(&record.created_at)?)
    .bind(json(record)?)
    .bind(project_partition_key)
    .execute(&mut **tx)
    .await
    .map_err(|error| error.to_string())?;
    if result.rows_affected() == 0 {
        return Err(
            "record id is already owned by another thread in this tenant/source scope, or the thread scope does not exist"
                .to_string(),
        );
    }
    Ok(())
}

fn make_record(
    thread_id: &str,
    tenant_id: &str,
    source_id: &str,
    input: UpsertRecordInput,
) -> EngineRecord {
    EngineRecord {
        id: input.id,
        thread_id: thread_id.to_string(),
        tenant_id: tenant_id.to_string(),
        source_id: source_id.to_string(),
        external_record_id: input.external_record_id,
        role: input.role,
        record_type: input.record_type,
        content: input.content,
        structured_payload: input.structured_payload,
        metadata: input.metadata,
        summary_status: input
            .summary_status
            .unwrap_or_else(|| "pending".to_string()),
        summary_id: input.summary_id,
        summarized_at: input.summarized_at,
        created_at: input.created_at,
    }
}

fn merge_delta(
    delta: &mut SummaryQueueDelta,
    previous: Option<&EngineRecord>,
    next: &EngineRecord,
) {
    let old_pending = previous
        .map(|r| summary_status_is_pending(Some(&r.summary_status)))
        .unwrap_or(false);
    let new_pending = summary_status_is_pending(Some(&next.summary_status));
    let old_tokens = previous
        .filter(|_| old_pending)
        .map(estimate_pending_record_tokens)
        .unwrap_or(0);
    let new_tokens = if new_pending {
        estimate_record_summary_tokens(
            &next.created_at,
            &next.role,
            &next.content,
            next.structured_payload.as_ref(),
            next.metadata.as_ref(),
        )
    } else {
        0
    };
    delta.count += i64::from(new_pending) - i64::from(old_pending);
    delta.tokens += new_tokens - old_tokens;
}

pub async fn delete_records_by_thread(
    db: &Db,
    thread_id: &str,
    tenant_id: &str,
    source_id: &str,
    record_type: Option<&str>,
) -> Result<i64, String> {
    let mut sql = "DELETE FROM engine_records WHERE tenant_id=$1 AND source_id=$2 AND thread_id=$3"
        .to_string();
    let normalized = record_type.map(str::trim).filter(|v| !v.is_empty());
    if normalized.is_some() {
        sql.push_str(" AND record_type=$4");
    }
    let mut query = sqlx::query(sqlx::AssertSqlSafe(sql))
        .bind(tenant_id)
        .bind(source_id)
        .bind(thread_id);
    if let Some(value) = normalized {
        query = query.bind(value);
    }
    let count = query
        .execute(db)
        .await
        .map_err(|e| e.to_string())?
        .rows_affected() as i64;
    compact_turns::delete_compact_turns_by_thread(db, thread_id, tenant_id, source_id, record_type)
        .await?;
    if count > 0 {
        threads::refresh_summary_queue_state(db, tenant_id, source_id, thread_id).await?;
    }
    Ok(count)
}

pub async fn delete_record_by_id(
    db: &Db,
    record_id: &str,
    tenant_id: &str,
    source_id: &str,
    thread_id: Option<&str>,
) -> Result<bool, String> {
    let normalized = thread_id.map(str::trim).filter(|v| !v.is_empty());
    let sql = if normalized.is_some() {
        "DELETE FROM engine_records WHERE id=$1 AND tenant_id=$2 AND source_id=$3 AND thread_id=$4 RETURNING data"
    } else {
        "DELETE FROM engine_records WHERE id=$1 AND tenant_id=$2 AND source_id=$3 RETURNING data"
    };
    let mut query = sqlx::query_scalar::<_, Json<serde_json::Value>>(sql)
        .bind(record_id)
        .bind(tenant_id)
        .bind(source_id);
    if let Some(value) = normalized {
        query = query.bind(value);
    }
    let deleted = query
        .fetch_optional(db)
        .await
        .map_err(|e| e.to_string())?
        .map(decode::<EngineRecord>)
        .transpose()?;
    if let Some(record) = deleted.as_ref() {
        compact_turns::rebuild_compact_turn_for_record(
            db,
            &record.thread_id,
            &record.tenant_id,
            &record.source_id,
            &record.record_type,
            record.metadata.as_ref(),
        )
        .await?;
        if summary_status_is_pending(Some(&record.summary_status)) {
            threads::apply_summary_queue_state_delta(
                db,
                &record.tenant_id,
                &record.source_id,
                &record.thread_id,
                -1,
                -estimate_pending_record_tokens(record),
            )
            .await?;
        }
    }
    Ok(deleted.is_some())
}
