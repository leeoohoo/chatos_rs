// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde::Deserialize;
use sqlx::{Postgres, Transaction};

use crate::db::Db;

#[derive(Debug, Clone, Deserialize, sqlx::FromRow, PartialEq, Eq)]
pub struct SubjectMemoryScopeDispatchOutbox {
    pub id: String,
    pub tenant_id: String,
    pub source_id: String,
    pub scope_key: String,
    pub subject_memory_dispatch_version: i64,
    pub subject_memory_dispatch_published_version: i64,
    pub subject_memory_dispatch_consumed_version: i64,
    pub subject_memory_dispatch_pending: bool,
}

pub async fn get_pending_subject_memory_dispatch(
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    scope_key: &str,
) -> Result<Option<SubjectMemoryScopeDispatchOutbox>, String> {
    load_dispatch(db, tenant_id, source_id, scope_key, true).await
}

pub async fn get_subject_memory_dispatch_state(
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    scope_key: &str,
) -> Result<Option<SubjectMemoryScopeDispatchOutbox>, String> {
    load_dispatch(db, tenant_id, source_id, scope_key, false).await
}

pub async fn claim_pending_subject_memory_dispatches(
    db: &Db,
    limit: i64,
) -> Result<Vec<SubjectMemoryScopeDispatchOutbox>, String> {
    sqlx::query_as::<_, SubjectMemoryScopeDispatchOutbox>(
        "WITH candidates AS (SELECT tenant_id,source_id,id FROM engine_subject_memory_scopes \
         WHERE subject_memory_dispatch_pending ORDER BY subject_memory_dispatch_requested_at,updated_at \
         LIMIT $1 FOR UPDATE SKIP LOCKED) UPDATE engine_subject_memory_scopes s SET \
         subject_memory_dispatch_published_version=GREATEST(s.subject_memory_dispatch_published_version,s.subject_memory_dispatch_version), \
         subject_memory_dispatch_published_at=now(),subject_memory_dispatch_last_error=NULL, \
         subject_memory_dispatch_pending=false FROM candidates c WHERE s.tenant_id=c.tenant_id \
         AND s.source_id=c.source_id AND s.id=c.id \
         RETURNING s.id,s.tenant_id,s.source_id,s.scope_key,s.subject_memory_dispatch_version, \
         s.subject_memory_dispatch_published_version,s.subject_memory_dispatch_consumed_version, \
         s.subject_memory_dispatch_pending",
    )
    .bind(limit.clamp(1, 10_000))
    .fetch_all(db)
    .await
    .map_err(|error| error.to_string())
}

pub async fn defer_subject_memory_dispatch(
    db: &Db,
    event: &SubjectMemoryScopeDispatchOutbox,
) -> Result<bool, String> {
    sqlx::query(
        "UPDATE engine_subject_memory_scopes SET subject_memory_dispatch_pending=true \
         WHERE tenant_id=$1 AND source_id=$2 AND scope_key=$3 AND subject_memory_dispatch_version>=$4",
    )
    .bind(&event.tenant_id)
    .bind(&event.source_id)
    .bind(&event.scope_key)
    .bind(event.subject_memory_dispatch_version)
    .execute(db)
    .await
    .map(|result| result.rows_affected() > 0)
    .map_err(|error| error.to_string())
}

pub async fn recover_stale_subject_memory_dispatches(
    db: &Db,
    stale_before: &str,
    limit: i64,
) -> Result<u64, String> {
    let result = sqlx::query(
        "WITH candidates AS (SELECT tenant_id,source_id,id FROM engine_subject_memory_scopes \
         WHERE NOT subject_memory_dispatch_pending \
         AND subject_memory_dispatch_consumed_version<subject_memory_dispatch_version \
         AND subject_memory_dispatch_published_at<=$1 \
         AND COALESCE(subject_memory_dispatch_dead_letter_version,-1)<subject_memory_dispatch_version \
         ORDER BY subject_memory_dispatch_published_at,updated_at LIMIT $2 FOR UPDATE SKIP LOCKED) \
         UPDATE engine_subject_memory_scopes s SET subject_memory_dispatch_pending=true, \
         subject_memory_dispatch_last_error=NULL FROM candidates c WHERE s.tenant_id=c.tenant_id \
         AND s.source_id=c.source_id AND s.id=c.id",
    )
    .bind(crate::repositories::postgres::timestamp(stale_before)?)
    .bind(limit.clamp(1, 10_000))
    .execute(db)
    .await
    .map_err(|error| error.to_string())?;
    Ok(result.rows_affected())
}

pub async fn mark_subject_memory_dispatch_consumed(
    db: &Db,
    event: &SubjectMemoryScopeDispatchOutbox,
) -> Result<bool, String> {
    sqlx::query(
        "UPDATE engine_subject_memory_scopes SET subject_memory_dispatch_consumed_version= \
         GREATEST(subject_memory_dispatch_consumed_version,$4),subject_memory_dispatch_consumed_at=now(), \
         subject_memory_dispatch_last_error=NULL WHERE tenant_id=$1 AND source_id=$2 AND scope_key=$3 \
         AND subject_memory_dispatch_version>=$4",
    ).bind(&event.tenant_id).bind(&event.source_id).bind(&event.scope_key)
      .bind(event.subject_memory_dispatch_version).execute(db).await
      .map(|result| result.rows_affected() > 0).map_err(|error| error.to_string())
}

pub async fn mark_subject_memory_dispatch_failed(
    db: &Db,
    event: &SubjectMemoryScopeDispatchOutbox,
    error: &str,
) -> Result<bool, String> {
    update_dispatch_error(db, event, error, false).await
}

pub async fn rearm_subject_memory_dispatch(
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    scope_key: &str,
) -> Result<Option<SubjectMemoryScopeDispatchOutbox>, String> {
    let existing = get_subject_memory_dispatch_state(db, tenant_id, source_id, scope_key).await?;
    let Some(existing) = existing else {
        return Ok(None);
    };
    if existing.subject_memory_dispatch_consumed_version < existing.subject_memory_dispatch_version
    {
        return Ok(Some(existing));
    }
    sqlx::query_as::<_, SubjectMemoryScopeDispatchOutbox>(
        "UPDATE engine_subject_memory_scopes SET subject_memory_dispatch_version=subject_memory_dispatch_version+1, \
         subject_memory_dispatch_requested_at=now(),subject_memory_dispatch_last_error=NULL, \
         subject_memory_dispatch_pending=true WHERE tenant_id=$1 AND source_id=$2 AND scope_key=$3 \
         AND status='active' AND subject_memory_dispatch_consumed_version>=subject_memory_dispatch_version \
         AND COALESCE(subject_memory_dispatch_dead_letter_version,-1)<subject_memory_dispatch_version \
         RETURNING id,tenant_id,source_id,scope_key,subject_memory_dispatch_version, \
         subject_memory_dispatch_published_version,subject_memory_dispatch_consumed_version, \
         subject_memory_dispatch_pending",
    ).bind(tenant_id).bind(source_id).bind(scope_key).fetch_optional(db).await.map_err(|error| error.to_string())
}

pub async fn replay_dead_lettered_subject_memory_dispatch(
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    scope_key: &str,
    dead_letter_version: i64,
) -> Result<Option<SubjectMemoryScopeDispatchOutbox>, String> {
    let mut tx = db.begin().await.map_err(|error| error.to_string())?;
    let replayed = replay_dead_lettered_subject_memory_dispatch_in_tx(
        &mut tx,
        tenant_id,
        source_id,
        scope_key,
        dead_letter_version,
    )
    .await?;
    tx.commit().await.map_err(|error| error.to_string())?;
    Ok(replayed)
}

pub async fn replay_dead_lettered_subject_memory_dispatch_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: &str,
    source_id: &str,
    scope_key: &str,
    dead_letter_version: i64,
) -> Result<Option<SubjectMemoryScopeDispatchOutbox>, String> {
    sqlx::query_as::<_, SubjectMemoryScopeDispatchOutbox>(
        "UPDATE engine_subject_memory_scopes SET subject_memory_dispatch_version=subject_memory_dispatch_version+1, \
         subject_memory_dispatch_requested_at=now(),subject_memory_dispatch_last_error=NULL, \
         subject_memory_dispatch_pending=true,subject_memory_dispatch_dead_letter_version=NULL, \
         subject_memory_dispatch_dead_lettered_at=NULL,subject_memory_dispatch_last_failed_at=NULL \
         WHERE tenant_id=$1 AND source_id=$2 AND scope_key=$3 AND status='active' \
         AND subject_memory_dispatch_version=$4 AND subject_memory_dispatch_dead_letter_version=$4 \
         AND subject_memory_dispatch_consumed_version>=$4 AND NOT subject_memory_dispatch_pending \
         RETURNING id,tenant_id,source_id,scope_key,subject_memory_dispatch_version, \
         subject_memory_dispatch_published_version,subject_memory_dispatch_consumed_version, \
         subject_memory_dispatch_pending",
    ).bind(tenant_id).bind(source_id).bind(scope_key).bind(dead_letter_version)
      .fetch_optional(&mut **tx).await.map_err(|error| error.to_string())
}

async fn load_dispatch(
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    scope_key: &str,
    pending_only: bool,
) -> Result<Option<SubjectMemoryScopeDispatchOutbox>, String> {
    sqlx::query_as::<_, SubjectMemoryScopeDispatchOutbox>(
        "SELECT id,tenant_id,source_id,scope_key,subject_memory_dispatch_version, \
         subject_memory_dispatch_published_version,subject_memory_dispatch_consumed_version, \
         subject_memory_dispatch_pending FROM engine_subject_memory_scopes \
         WHERE tenant_id=$1 AND source_id=$2 AND scope_key=$3 \
         AND (NOT $4 OR subject_memory_dispatch_pending)",
    )
    .bind(tenant_id)
    .bind(source_id)
    .bind(scope_key)
    .bind(pending_only)
    .fetch_optional(db)
    .await
    .map_err(|error| error.to_string())
}

async fn update_dispatch_error(
    db: &Db,
    event: &SubjectMemoryScopeDispatchOutbox,
    error: &str,
    dead_letter: bool,
) -> Result<bool, String> {
    let query = if dead_letter {
        "UPDATE engine_subject_memory_scopes SET subject_memory_dispatch_consumed_version= \
         GREATEST(subject_memory_dispatch_consumed_version,$4),subject_memory_dispatch_dead_letter_version=$4, \
         subject_memory_dispatch_dead_lettered_at=now(),subject_memory_dispatch_last_error=$5 \
         WHERE tenant_id=$1 AND source_id=$2 AND scope_key=$3 AND subject_memory_dispatch_version>=$4"
    } else {
        "UPDATE engine_subject_memory_scopes SET subject_memory_dispatch_last_error=$5, \
         subject_memory_dispatch_last_failed_at=now() WHERE tenant_id=$1 AND source_id=$2 \
         AND scope_key=$3 AND subject_memory_dispatch_version>=$4"
    };
    sqlx::query(query)
        .bind(&event.tenant_id)
        .bind(&event.source_id)
        .bind(&event.scope_key)
        .bind(event.subject_memory_dispatch_version)
        .bind(error)
        .execute(db)
        .await
        .map(|result| result.rows_affected() > 0)
        .map_err(|error| error.to_string())
}
