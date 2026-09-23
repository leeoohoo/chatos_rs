// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde::Deserialize;
use sqlx::types::Json;
use sqlx::{Postgres, QueryBuilder};
use uuid::Uuid;

use crate::db::Db;
use crate::models::{now_rfc3339, EngineSubjectMemoryScope, UpsertSubjectMemoryScopeRequest};
use crate::repositories::postgres::{decode, json, timestamp};

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

pub async fn upsert_subject_memory_scope(
    db: &Db,
    scope_key: &str,
    req: UpsertSubjectMemoryScopeRequest,
) -> Result<EngineSubjectMemoryScope, String> {
    let scope_key = scope_key.trim();
    if scope_key.is_empty() {
        return Err("empty scope_key".to_string());
    }
    let existing = get_subject_memory_scope(db, &req.tenant_id, &req.source_id, scope_key).await?;
    let now = now_rfc3339();
    let status = req.status.unwrap_or_else(|| "active".to_string());
    let active = status == "active";
    let scope = EngineSubjectMemoryScope {
        id: existing
            .as_ref()
            .map(|item| item.id.clone())
            .unwrap_or_else(|| format!("sms_{}", Uuid::new_v4())),
        tenant_id: req.tenant_id,
        source_id: req.source_id,
        scope_key: scope_key.to_string(),
        subject_id: req.subject_id,
        memory_type: req.memory_type,
        source_thread_label: req.source_thread_label,
        relation_subject_id: req.relation_subject_id,
        source_summary_type: req.source_summary_type,
        prompt_title: req.prompt_title,
        memory_metadata: req.memory_metadata,
        status: status.clone(),
        created_at: existing
            .as_ref()
            .map(|item| item.created_at.clone())
            .unwrap_or_else(|| now.clone()),
        updated_at: now.clone(),
        last_run_at: existing.and_then(|item| item.last_run_at),
    };
    let requested_at = active.then(|| timestamp(&now)).transpose()?;
    sqlx::query(
        "INSERT INTO engine_subject_memory_scopes \
         (id,tenant_id,source_id,scope_key,subject_id,memory_type,source_thread_label, \
          relation_subject_id,source_summary_type,status,subject_memory_status, \
          subject_memory_dispatch_pending,subject_memory_dispatch_version, \
          subject_memory_dispatch_requested_at,created_at,updated_at,data) \
         VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,'idle',$11,$12,$13,$14,$15,$16) \
         ON CONFLICT(tenant_id,source_id,scope_key) DO UPDATE SET subject_id=EXCLUDED.subject_id, \
         memory_type=EXCLUDED.memory_type,source_thread_label=EXCLUDED.source_thread_label, \
         relation_subject_id=EXCLUDED.relation_subject_id,source_summary_type=EXCLUDED.source_summary_type, \
         status=EXCLUDED.status,subject_memory_dispatch_pending=EXCLUDED.subject_memory_dispatch_pending, \
         subject_memory_dispatch_version=engine_subject_memory_scopes.subject_memory_dispatch_version + \
             CASE WHEN EXCLUDED.status='active' THEN 1 ELSE 0 END, \
         subject_memory_dispatch_requested_at=CASE WHEN EXCLUDED.status='active' \
             THEN EXCLUDED.subject_memory_dispatch_requested_at \
             ELSE engine_subject_memory_scopes.subject_memory_dispatch_requested_at END, \
         subject_memory_dispatch_last_error=CASE WHEN EXCLUDED.status='active' THEN NULL \
             ELSE engine_subject_memory_scopes.subject_memory_dispatch_last_error END, \
         updated_at=EXCLUDED.updated_at,data=EXCLUDED.data",
    )
    .bind(&scope.id)
    .bind(&scope.tenant_id)
    .bind(&scope.source_id)
    .bind(&scope.scope_key)
    .bind(&scope.subject_id)
    .bind(&scope.memory_type)
    .bind(&scope.source_thread_label)
    .bind(&scope.relation_subject_id)
    .bind(&scope.source_summary_type)
    .bind(&scope.status)
    .bind(active)
    .bind(if active { 1_i64 } else { 0 })
    .bind(requested_at)
    .bind(timestamp(&scope.created_at)?)
    .bind(timestamp(&scope.updated_at)?)
    .bind(json(&scope)?)
    .execute(db)
    .await
    .map_err(|error| error.to_string())?;
    Ok(scope)
}

pub async fn get_subject_memory_scope(
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    scope_key: &str,
) -> Result<Option<EngineSubjectMemoryScope>, String> {
    sqlx::query_scalar::<_, Json<serde_json::Value>>(
        "SELECT data FROM engine_subject_memory_scopes \
         WHERE tenant_id=$1 AND source_id=$2 AND scope_key=$3",
    )
    .bind(tenant_id)
    .bind(source_id)
    .bind(scope_key)
    .fetch_optional(db)
    .await
    .map_err(|error| error.to_string())?
    .map(decode)
    .transpose()
}

pub async fn list_active_subject_memory_scopes(
    db: &Db,
    tenant_id: Option<&str>,
    source_id: Option<&str>,
    limit: i64,
) -> Result<Vec<EngineSubjectMemoryScope>, String> {
    list_active_subject_memory_scopes_page(db, tenant_id, source_id, limit, 0).await
}

pub async fn list_active_subject_memory_scopes_page(
    db: &Db,
    tenant_id: Option<&str>,
    source_id: Option<&str>,
    limit: i64,
    offset: u64,
) -> Result<Vec<EngineSubjectMemoryScope>, String> {
    let mut query = QueryBuilder::<Postgres>::new(
        "SELECT data FROM engine_subject_memory_scopes WHERE status='active'",
    );
    for (column, value) in [
        ("tenant_id", normalized(tenant_id)),
        ("source_id", normalized(source_id)),
    ] {
        if let Some(value) = value {
            query.push(" AND ").push(column).push("=").push_bind(value);
        }
    }
    query
        .push(" ORDER BY updated_at DESC,created_at DESC LIMIT ")
        .push_bind(limit.clamp(1, 10_000))
        .push(" OFFSET ")
        .push_bind(i64::try_from(offset).unwrap_or(i64::MAX));
    decode_many(
        query
            .build_query_scalar()
            .fetch_all(db)
            .await
            .map_err(|error| error.to_string())?,
    )
}

pub async fn list_matching_active_subject_memory_scopes(
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    thread_labels: &[String],
    summary_type: &str,
    limit: i64,
) -> Result<Vec<EngineSubjectMemoryScope>, String> {
    if thread_labels.is_empty() {
        return Ok(Vec::new());
    }
    let allow_default = summary_type.trim() == "thread_incremental";
    let rows = sqlx::query_scalar::<_, Json<serde_json::Value>>(
        "SELECT data FROM engine_subject_memory_scopes WHERE tenant_id=$1 AND source_id=$2 \
         AND status='active' AND source_thread_label=ANY($3) AND \
         (source_summary_type=$4 OR ($5 AND COALESCE(source_summary_type,'')='')) \
         ORDER BY updated_at DESC,created_at DESC LIMIT $6",
    )
    .bind(tenant_id)
    .bind(source_id)
    .bind(thread_labels)
    .bind(summary_type.trim())
    .bind(allow_default)
    .bind(limit.clamp(1, 10_000))
    .fetch_all(db)
    .await
    .map_err(|error| error.to_string())?;
    decode_many(rows)
}

pub async fn touch_subject_memory_scope_run(
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    scope_key: &str,
) -> Result<(), String> {
    let now = now_rfc3339();
    sqlx::query(
        "UPDATE engine_subject_memory_scopes SET updated_at=$4, \
         data=jsonb_set(jsonb_set(data,'{last_run_at}',to_jsonb($5::text)), \
         '{updated_at}',to_jsonb($5::text)) WHERE tenant_id=$1 AND source_id=$2 AND scope_key=$3",
    )
    .bind(tenant_id)
    .bind(source_id)
    .bind(scope_key)
    .bind(timestamp(&now)?)
    .bind(&now)
    .execute(db)
    .await
    .map_err(|error| error.to_string())?;
    Ok(())
}

pub async fn try_acquire_subject_memory_scope_slot(
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    scope_key: &str,
    lock_owner: &str,
    lock_timeout_secs: i64,
) -> Result<bool, String> {
    let now = chrono::Utc::now();
    let expires_at = now + chrono::Duration::seconds(lock_timeout_secs.max(30));
    sqlx::query(
        "UPDATE engine_subject_memory_scopes SET subject_memory_status='running',lock_owner=$4, \
         lock_expires_at=$5,updated_at=$6 WHERE tenant_id=$1 AND source_id=$2 AND scope_key=$3 \
         AND status='active' AND (subject_memory_status<>'running' OR lock_expires_at<=$6 OR lock_owner=$4)",
    )
    .bind(tenant_id).bind(source_id).bind(scope_key).bind(lock_owner).bind(expires_at).bind(now)
    .execute(db).await.map(|result| result.rows_affected() > 0).map_err(|error| error.to_string())
}

pub async fn release_subject_memory_scope_slot(
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    scope_key: &str,
    lock_owner: &str,
) -> Result<(), String> {
    sqlx::query(
        "UPDATE engine_subject_memory_scopes SET subject_memory_status='idle',lock_owner=NULL, \
         lock_expires_at=NULL,updated_at=now() WHERE tenant_id=$1 AND source_id=$2 \
         AND scope_key=$3 AND lock_owner=$4",
    )
    .bind(tenant_id)
    .bind(source_id)
    .bind(scope_key)
    .bind(lock_owner)
    .execute(db)
    .await
    .map_err(|error| error.to_string())?;
    Ok(())
}

pub async fn refresh_subject_memory_scope_slot(
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    scope_key: &str,
    lock_owner: &str,
    lock_timeout_secs: i64,
) -> Result<bool, String> {
    let expires_at = chrono::Utc::now() + chrono::Duration::seconds(lock_timeout_secs.max(30));
    sqlx::query(
        "UPDATE engine_subject_memory_scopes SET lock_expires_at=$5,updated_at=now() \
         WHERE tenant_id=$1 AND source_id=$2 AND scope_key=$3 \
         AND subject_memory_status='running' AND lock_owner=$4",
    )
    .bind(tenant_id)
    .bind(source_id)
    .bind(scope_key)
    .bind(lock_owner)
    .bind(expires_at)
    .execute(db)
    .await
    .map(|result| result.rows_affected() > 0)
    .map_err(|error| error.to_string())
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

pub async fn list_pending_subject_memory_dispatches(
    db: &Db,
    limit: i64,
) -> Result<Vec<SubjectMemoryScopeDispatchOutbox>, String> {
    sqlx::query_as::<_, SubjectMemoryScopeDispatchOutbox>(
        "SELECT id,tenant_id,source_id,scope_key,subject_memory_dispatch_version, \
         subject_memory_dispatch_published_version,subject_memory_dispatch_consumed_version, \
         subject_memory_dispatch_pending FROM engine_subject_memory_scopes \
         WHERE subject_memory_dispatch_pending ORDER BY subject_memory_dispatch_requested_at,updated_at \
         LIMIT $1",
    ).bind(limit.clamp(1, 10_000)).fetch_all(db).await.map_err(|error| error.to_string())
}

pub async fn mark_subject_memory_dispatch_published(
    db: &Db,
    event: &SubjectMemoryScopeDispatchOutbox,
) -> Result<bool, String> {
    sqlx::query(
        "UPDATE engine_subject_memory_scopes SET subject_memory_dispatch_published_version= \
         GREATEST(subject_memory_dispatch_published_version,$4),subject_memory_dispatch_published_at=now(), \
         subject_memory_dispatch_last_error=NULL,subject_memory_dispatch_pending=subject_memory_dispatch_version>$4 \
         WHERE tenant_id=$1 AND source_id=$2 AND scope_key=$3 AND subject_memory_dispatch_version>=$4",
    ).bind(&event.tenant_id).bind(&event.source_id).bind(&event.scope_key)
      .bind(event.subject_memory_dispatch_version).execute(db).await
      .map(|result| result.rows_affected() > 0).map_err(|error| error.to_string())
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

pub async fn mark_subject_memory_dispatch_dead_lettered(
    db: &Db,
    event: &SubjectMemoryScopeDispatchOutbox,
    error: &str,
) -> Result<bool, String> {
    update_dispatch_error(db, event, error, true).await
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
      .fetch_optional(db).await.map_err(|error| error.to_string())
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

fn decode_many(
    rows: Vec<Json<serde_json::Value>>,
) -> Result<Vec<EngineSubjectMemoryScope>, String> {
    rows.into_iter().map(decode).collect()
}

fn normalized(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}
