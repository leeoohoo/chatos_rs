// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde::Deserialize;
use sqlx::FromRow;

use crate::db::Db;
use crate::repositories::postgres::timestamp;

#[derive(Debug, Clone, Deserialize, FromRow, PartialEq, Eq)]
pub struct SummaryDispatchOutbox {
    pub tenant_id: String,
    pub source_id: String,
    pub thread_id: String,
    pub summary_dispatch_version: i64,
    pub summary_dispatch_published_version: i64,
    pub summary_dispatch_consumed_version: i64,
}

const COLUMNS: &str = "tenant_id,source_id,id AS thread_id,summary_dispatch_version,summary_dispatch_published_version,summary_dispatch_consumed_version";

pub async fn get_pending_summary_dispatch(
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    thread_id: &str,
) -> Result<Option<SummaryDispatchOutbox>, String> {
    fetch_optional(db, &format!(
        "SELECT {COLUMNS} FROM engine_threads WHERE tenant_id=$1 AND source_id=$2 AND id=$3 \
         AND summary_dispatch_pending AND (summary_status<>'running' OR summary_lock_expires_at IS NULL OR summary_lock_expires_at<=now())"
    ), tenant_id, source_id, thread_id).await
}

pub async fn get_summary_dispatch_state(
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    thread_id: &str,
) -> Result<Option<SummaryDispatchOutbox>, String> {
    fetch_optional(
        db,
        &format!(
            "SELECT {COLUMNS} FROM engine_threads WHERE tenant_id=$1 AND source_id=$2 AND id=$3"
        ),
        tenant_id,
        source_id,
        thread_id,
    )
    .await
}

pub async fn list_pending_summary_dispatches(
    db: &Db,
    limit: i64,
) -> Result<Vec<SummaryDispatchOutbox>, String> {
    sqlx::query_as::<_, SummaryDispatchOutbox>(&format!(
        "SELECT {COLUMNS} FROM engine_threads WHERE summary_dispatch_pending \
         AND (summary_status<>'running' OR summary_lock_expires_at IS NULL OR summary_lock_expires_at<=now()) \
         ORDER BY summary_dispatch_requested_at,updated_at LIMIT $1"
    )).bind(limit.clamp(1,10_000)).fetch_all(db).await.map_err(|error| error.to_string())
}

pub async fn list_eligible_summary_dispatches(
    db: &Db,
    token_threshold: i64,
    limit: i64,
) -> Result<Vec<SummaryDispatchOutbox>, String> {
    sqlx::query_as::<_, SummaryDispatchOutbox>(&format!(
        "SELECT {COLUMNS} FROM engine_threads WHERE summary_status='pending' AND pending_summary_tokens>=$1 \
         AND summary_dispatch_consumed_version>=summary_dispatch_version \
         AND COALESCE(summary_dispatch_dead_letter_version,-1)<summary_dispatch_version \
         ORDER BY updated_at LIMIT $2"
    )).bind(token_threshold.max(1)).bind(limit.clamp(1,10_000)).fetch_all(db).await.map_err(|error| error.to_string())
}

pub async fn list_stale_published_summary_dispatches(
    db: &Db,
    token_threshold: i64,
    stale_before: &str,
    limit: i64,
) -> Result<Vec<SummaryDispatchOutbox>, String> {
    sqlx::query_as::<_, SummaryDispatchOutbox>(&format!(
        "SELECT {COLUMNS} FROM engine_threads WHERE summary_status='pending' AND pending_summary_tokens>=$1 \
         AND NOT summary_dispatch_pending AND summary_dispatch_published_at<=$2 AND summary_dispatch_version>0 \
         AND summary_dispatch_published_version>=summary_dispatch_version \
         AND summary_dispatch_consumed_version<summary_dispatch_version \
         AND COALESCE(summary_dispatch_dead_letter_version,-1)<summary_dispatch_version \
         ORDER BY summary_dispatch_published_at,updated_at LIMIT $3"
    )).bind(token_threshold.max(1)).bind(timestamp(stale_before)?).bind(limit.clamp(1,10_000))
      .fetch_all(db).await.map_err(|error| error.to_string())
}

pub async fn defer_summary_dispatch_until_unlock(
    db: &Db,
    event: &SummaryDispatchOutbox,
) -> Result<bool, String> {
    update_event(
        db,
        event,
        "summary_dispatch_pending=true,summary_dispatch_last_error=NULL",
    )
    .await
}

pub async fn mark_summary_dispatch_published(
    db: &Db,
    event: &SummaryDispatchOutbox,
) -> Result<bool, String> {
    update_event(
        db,
        event,
        "summary_dispatch_published_version=GREATEST(summary_dispatch_published_version,$4), \
         summary_dispatch_published_at=now(),summary_dispatch_last_error=NULL, \
         summary_dispatch_pending=(summary_dispatch_version>$4)",
    )
    .await
}

pub async fn mark_summary_dispatch_consumed(
    db: &Db,
    event: &SummaryDispatchOutbox,
) -> Result<bool, String> {
    update_event(
        db,
        event,
        "summary_dispatch_consumed_version=GREATEST(summary_dispatch_consumed_version,$4), \
         summary_dispatch_consumed_at=now(),summary_dispatch_last_error=NULL",
    )
    .await
}

pub async fn mark_summary_dispatch_failed(
    db: &Db,
    event: &SummaryDispatchOutbox,
    error: &str,
) -> Result<bool, String> {
    let result = sqlx::query(
        "UPDATE engine_threads SET summary_dispatch_last_error=$5,summary_dispatch_last_failed_at=now() \
         WHERE tenant_id=$1 AND source_id=$2 AND id=$3 AND summary_dispatch_version>=$4"
    ).bind(&event.tenant_id).bind(&event.source_id).bind(&event.thread_id)
      .bind(event.summary_dispatch_version).bind(error).execute(db).await.map_err(|value| value.to_string())?;
    Ok(result.rows_affected() > 0)
}

pub async fn mark_summary_dispatch_dead_lettered(
    db: &Db,
    event: &SummaryDispatchOutbox,
    error: &str,
) -> Result<bool, String> {
    let result = sqlx::query(
        "UPDATE engine_threads SET summary_dispatch_consumed_version=GREATEST(summary_dispatch_consumed_version,$4), \
         summary_dispatch_dead_letter_version=$4,summary_dispatch_dead_lettered_at=now(),summary_dispatch_last_error=$5 \
         WHERE tenant_id=$1 AND source_id=$2 AND id=$3 AND summary_dispatch_version>=$4"
    ).bind(&event.tenant_id).bind(&event.source_id).bind(&event.thread_id)
      .bind(event.summary_dispatch_version).bind(error).execute(db).await.map_err(|value| value.to_string())?;
    Ok(result.rows_affected() > 0)
}

pub async fn rearm_summary_dispatch_if_eligible(
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    thread_id: &str,
    token_threshold: i64,
) -> Result<Option<SummaryDispatchOutbox>, String> {
    sqlx::query_as::<_, SummaryDispatchOutbox>(&format!(
        "UPDATE engine_threads SET summary_dispatch_version=summary_dispatch_version+1, \
         summary_dispatch_requested_at=now(),summary_dispatch_last_error=NULL,summary_dispatch_pending=true \
         WHERE tenant_id=$1 AND source_id=$2 AND id=$3 AND summary_status='pending' AND pending_summary_tokens>=$4 \
         AND summary_dispatch_consumed_version>=summary_dispatch_version \
         AND COALESCE(summary_dispatch_dead_letter_version,-1)<summary_dispatch_version RETURNING {COLUMNS}"
    )).bind(tenant_id).bind(source_id).bind(thread_id).bind(token_threshold.max(1))
      .fetch_optional(db).await.map_err(|error| error.to_string())
}

pub async fn rearm_stale_published_summary_dispatch(
    db: &Db,
    event: &SummaryDispatchOutbox,
    token_threshold: i64,
    stale_before: &str,
) -> Result<Option<SummaryDispatchOutbox>, String> {
    sqlx::query_as::<_, SummaryDispatchOutbox>(&format!(
        "UPDATE engine_threads SET summary_dispatch_version=summary_dispatch_version+1, \
         summary_dispatch_recovery_count=summary_dispatch_recovery_count+1,summary_dispatch_requested_at=now(), \
         summary_dispatch_recovered_at=now(),summary_dispatch_last_error=NULL,summary_dispatch_pending=true \
         WHERE tenant_id=$1 AND source_id=$2 AND id=$3 AND summary_dispatch_version=$4 \
         AND summary_status='pending' AND pending_summary_tokens>=$5 AND NOT summary_dispatch_pending \
         AND summary_dispatch_published_at<=$6 AND summary_dispatch_version>0 \
         AND summary_dispatch_published_version>=summary_dispatch_version \
         AND summary_dispatch_consumed_version<summary_dispatch_version \
         AND COALESCE(summary_dispatch_dead_letter_version,-1)<summary_dispatch_version RETURNING {COLUMNS}"
    )).bind(&event.tenant_id).bind(&event.source_id).bind(&event.thread_id)
      .bind(event.summary_dispatch_version).bind(token_threshold.max(1)).bind(timestamp(stale_before)?)
      .fetch_optional(db).await.map_err(|error| error.to_string())
}

pub async fn replay_dead_lettered_summary_dispatch(
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    thread_id: &str,
    dead_letter_version: i64,
) -> Result<Option<SummaryDispatchOutbox>, String> {
    sqlx::query_as::<_, SummaryDispatchOutbox>(&format!(
        "UPDATE engine_threads SET summary_dispatch_version=summary_dispatch_version+1, \
         summary_dispatch_requested_at=now(),summary_dispatch_last_error=NULL,summary_dispatch_pending=true, \
         summary_dispatch_dead_letter_version=NULL,summary_dispatch_dead_lettered_at=NULL,summary_dispatch_last_failed_at=NULL \
         WHERE tenant_id=$1 AND source_id=$2 AND id=$3 AND summary_status='pending' \
         AND summary_dispatch_version=$4 AND summary_dispatch_dead_letter_version=$4 \
         AND summary_dispatch_consumed_version>=$4 AND NOT summary_dispatch_pending RETURNING {COLUMNS}"
    )).bind(tenant_id).bind(source_id).bind(thread_id).bind(dead_letter_version)
      .fetch_optional(db).await.map_err(|error| error.to_string())
}

async fn fetch_optional(
    db: &Db,
    query: &str,
    tenant_id: &str,
    source_id: &str,
    thread_id: &str,
) -> Result<Option<SummaryDispatchOutbox>, String> {
    sqlx::query_as(query)
        .bind(tenant_id)
        .bind(source_id)
        .bind(thread_id)
        .fetch_optional(db)
        .await
        .map_err(|error| error.to_string())
}

async fn update_event(
    db: &Db,
    event: &SummaryDispatchOutbox,
    assignments: &str,
) -> Result<bool, String> {
    let sql = format!("UPDATE engine_threads SET {assignments} WHERE tenant_id=$1 AND source_id=$2 AND id=$3 AND summary_dispatch_version>=$4");
    let result = sqlx::query(&sql)
        .bind(&event.tenant_id)
        .bind(&event.source_id)
        .bind(&event.thread_id)
        .bind(event.summary_dispatch_version)
        .execute(db)
        .await
        .map_err(|error| error.to_string())?;
    Ok(result.rows_affected() > 0)
}
