// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashSet;

use sqlx::types::Json;

use crate::config::AppConfig;
use crate::db::Db;
use crate::models::{BatchSyncRecordsRequest, EngineRecord, UpsertRecordInput};
use crate::repositories::postgres::{decode, json, timestamp};
use crate::repositories::threads;

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
    threads::begin_record_sync(
        db,
        &req.tenant_id,
        &req.source_id,
        thread_id,
        config.record_sync_lease_timeout_secs,
    )
    .await?;
    let result = batch_sync_records_inner(db, thread_id, req).await;
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
) -> Result<usize, String> {
    let mut tx = db.begin().await.map_err(|e| e.to_string())?;
    let mut inserted = 0usize;
    let mut delta = SummaryQueueDelta::default();
    let mut keys = HashSet::new();
    for input in &req.records {
        let previous=sqlx::query_scalar::<_,Json<serde_json::Value>>(
            "SELECT data FROM engine_records WHERE tenant_id=$1 AND source_id=$2 AND thread_id=$3 AND id=$4 FOR UPDATE"
        ).bind(&req.tenant_id).bind(&req.source_id).bind(thread_id).bind(&input.id)
          .fetch_optional(&mut *tx).await.map_err(|e|e.to_string())?.map(decode::<EngineRecord>).transpose()?;
        let record = make_record(thread_id, &req.tenant_id, &req.source_id, input.clone());
        inserted += usize::from(previous.is_none());
        merge_delta(&mut delta, previous.as_ref(), &record);
        if let Some(key) = CompactTurnKey::from_record(&record) {
            keys.insert(key);
        }
        if let Some(key) = previous.as_ref().and_then(CompactTurnKey::from_record) {
            keys.insert(key);
        }
        sqlx::query(
            "INSERT INTO engine_records(id,thread_id,tenant_id,source_id,external_record_id,role,record_type,summary_status,summary_id,created_at,data) \
             VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11) ON CONFLICT(id) DO UPDATE SET \
             external_record_id=EXCLUDED.external_record_id,role=EXCLUDED.role,record_type=EXCLUDED.record_type, \
             summary_status=EXCLUDED.summary_status,summary_id=EXCLUDED.summary_id,created_at=EXCLUDED.created_at,data=EXCLUDED.data"
        ).bind(&record.id).bind(&record.thread_id).bind(&record.tenant_id).bind(&record.source_id)
          .bind(&record.external_record_id).bind(&record.role).bind(&record.record_type).bind(&record.summary_status)
          .bind(&record.summary_id).bind(timestamp(&record.created_at)?).bind(json(&record)?)
          .execute(&mut *tx).await.map_err(|e|e.to_string())?;
    }
    tx.commit().await.map_err(|e| e.to_string())?;
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
    if delta.count != 0 || delta.tokens != 0 {
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
    Ok(inserted)
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
    let mut query = sqlx::query(&sql)
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
