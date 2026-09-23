// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use sqlx::types::Json;

use crate::db::Db;
use crate::models::{
    now_plus_seconds_rfc3339, now_rfc3339, DeleteThreadResponse, EngineRecord, EngineThread,
    UpsertThreadRequest,
};
use crate::repositories::postgres::{decode, json, optional_timestamp, timestamp};
use crate::repositories::records;

const SUMMARY_LOCK_TIMEOUT_SECS: i64 = 300;

pub async fn upsert_thread(
    db: &Db,
    thread_id: &str,
    req: UpsertThreadRequest,
) -> Result<EngineThread, String> {
    let existing = load_thread(db, &req.tenant_id, &req.source_id, thread_id).await?;
    let now = now_rfc3339();
    let updated_at = req.updated_at.unwrap_or_else(|| now.clone());
    let status = req.status.unwrap_or_else(|| "active".to_string());
    let thread = EngineThread {
        id: thread_id.to_string(),
        tenant_id: req.tenant_id,
        source_id: req.source_id,
        subject_id: req.subject_id,
        thread_type: req.thread_type,
        external_thread_id: req.external_thread_id,
        title: req.title,
        labels: req.labels,
        metadata: req.metadata,
        status: status.clone(),
        summary_status: existing
            .as_ref()
            .map(|item| item.summary_status.clone())
            .unwrap_or_else(|| "idle".to_string()),
        summary_job_run_id: existing
            .as_ref()
            .and_then(|item| item.summary_job_run_id.clone()),
        summary_locked_at: existing
            .as_ref()
            .and_then(|item| item.summary_locked_at.clone()),
        summary_lock_expires_at: existing
            .as_ref()
            .and_then(|item| item.summary_lock_expires_at.clone()),
        pending_record_count: existing
            .as_ref()
            .map(|item| item.pending_record_count)
            .unwrap_or(0),
        pending_summary_tokens: existing
            .as_ref()
            .map(|item| item.pending_summary_tokens)
            .unwrap_or(0),
        created_at: existing
            .as_ref()
            .map(|item| item.created_at.clone())
            .or(req.created_at)
            .unwrap_or_else(|| now.clone()),
        updated_at: updated_at.clone(),
        archived_at: req
            .archived_at
            .or_else(|| (status == "archived").then(|| updated_at.clone())),
    };
    sqlx::query(
        "INSERT INTO engine_threads \
         (id,tenant_id,source_id,subject_id,thread_type,external_thread_id,status,summary_status, \
          pending_record_count,pending_summary_tokens,summary_job_run_id,summary_locked_at, \
          summary_lock_expires_at,created_at,updated_at,archived_at,data) \
         VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17) \
         ON CONFLICT(id) DO UPDATE SET subject_id=EXCLUDED.subject_id,thread_type=EXCLUDED.thread_type, \
         external_thread_id=EXCLUDED.external_thread_id,status=EXCLUDED.status, \
         updated_at=EXCLUDED.updated_at,archived_at=EXCLUDED.archived_at,data=EXCLUDED.data",
    )
    .bind(&thread.id)
    .bind(&thread.tenant_id)
    .bind(&thread.source_id)
    .bind(&thread.subject_id)
    .bind(&thread.thread_type)
    .bind(&thread.external_thread_id)
    .bind(&thread.status)
    .bind(&thread.summary_status)
    .bind(thread.pending_record_count)
    .bind(thread.pending_summary_tokens)
    .bind(&thread.summary_job_run_id)
    .bind(optional_timestamp(thread.summary_locked_at.as_deref())?)
    .bind(optional_timestamp(thread.summary_lock_expires_at.as_deref())?)
    .bind(timestamp(&thread.created_at)?)
    .bind(timestamp(&thread.updated_at)?)
    .bind(optional_timestamp(thread.archived_at.as_deref())?)
    .bind(json(&thread)?)
    .execute(db)
    .await
    .map_err(|error| error.to_string())?;
    Ok(thread)
}

pub async fn delete_thread(
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    thread_id: &str,
) -> Result<DeleteThreadResponse, String> {
    let mut tx = db.begin().await.map_err(|error| error.to_string())?;
    let deleted_records =
        count_scope(&mut tx, "engine_records", tenant_id, source_id, thread_id).await?;
    let deleted_summaries =
        count_scope(&mut tx, "engine_summaries", tenant_id, source_id, thread_id).await?;
    let deleted_snapshots = count_scope(
        &mut tx,
        "engine_thread_snapshots",
        tenant_id,
        source_id,
        thread_id,
    )
    .await?;
    let deleted_thread =
        sqlx::query("DELETE FROM engine_threads WHERE tenant_id=$1 AND source_id=$2 AND id=$3")
            .bind(tenant_id)
            .bind(source_id)
            .bind(thread_id)
            .execute(&mut *tx)
            .await
            .map_err(|error| error.to_string())?
            .rows_affected()
            > 0;
    tx.commit().await.map_err(|error| error.to_string())?;
    Ok(DeleteThreadResponse {
        deleted_thread,
        deleted_records,
        deleted_summaries,
        deleted_snapshots,
    })
}

pub async fn refresh_summary_queue_state(
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    thread_id: &str,
) -> Result<Option<EngineThread>, String> {
    let rows = sqlx::query_scalar::<_, Json<serde_json::Value>>(
        "SELECT data FROM engine_records WHERE tenant_id=$1 AND source_id=$2 AND thread_id=$3 \
         AND summary_status='pending' ORDER BY created_at,id",
    )
    .bind(tenant_id)
    .bind(source_id)
    .bind(thread_id)
    .fetch_all(db)
    .await
    .map_err(|error| error.to_string())?;
    let records = rows
        .into_iter()
        .map(decode::<EngineRecord>)
        .collect::<Result<Vec<_>, _>>()?;
    mutate_thread(db, tenant_id, source_id, thread_id, false, |thread| {
        if thread.summary_status == "running" {
            return false;
        }
        thread.pending_record_count = records.len() as i64;
        thread.pending_summary_tokens = records
            .iter()
            .map(records::estimate_pending_record_tokens)
            .sum();
        thread.summary_status = if records.is_empty() {
            "idle"
        } else {
            "pending"
        }
        .to_string();
        true
    })
    .await
}

pub async fn apply_summary_queue_state_delta(
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    thread_id: &str,
    pending_record_count_delta: i64,
    pending_summary_tokens_delta: i64,
) -> Result<Option<EngineThread>, String> {
    if pending_record_count_delta == 0 && pending_summary_tokens_delta == 0 {
        return Ok(None);
    }
    mutate_thread(db, tenant_id, source_id, thread_id, false, |thread| {
        thread.pending_record_count =
            (thread.pending_record_count + pending_record_count_delta).max(0);
        thread.pending_summary_tokens =
            (thread.pending_summary_tokens + pending_summary_tokens_delta).max(0);
        if thread.summary_status != "running" {
            thread.summary_status = if thread.pending_record_count > 0 {
                "pending"
            } else {
                "idle"
            }
            .to_string();
        }
        true
    })
    .await
}

pub async fn begin_record_sync(
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    thread_id: &str,
    lease_timeout_secs: i64,
) -> Result<(), String> {
    let now = now_rfc3339();
    let expires = now_plus_seconds_rfc3339(lease_timeout_secs.max(30));
    let result = sqlx::query(
        "UPDATE engine_threads SET record_sync_inflight=record_sync_inflight+1, \
         record_sync_lease_expires_at=$4,updated_at=$5, \
         data=jsonb_set(data,'{updated_at}',to_jsonb($6::text),true) \
         WHERE tenant_id=$1 AND source_id=$2 AND id=$3",
    )
    .bind(tenant_id)
    .bind(source_id)
    .bind(thread_id)
    .bind(timestamp(&expires)?)
    .bind(timestamp(&now)?)
    .bind(&now)
    .execute(db)
    .await
    .map_err(|error| error.to_string())?;
    if result.rows_affected() == 0 {
        return Err("thread not found for record sync".to_string());
    }
    Ok(())
}

pub async fn finish_record_sync(
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    thread_id: &str,
) -> Result<(), String> {
    let now = now_rfc3339();
    sqlx::query(
        "UPDATE engine_threads SET record_sync_inflight=GREATEST(record_sync_inflight-1,0), \
         record_sync_lease_expires_at=CASE WHEN record_sync_inflight-1>0 THEN record_sync_lease_expires_at ELSE NULL END, \
         updated_at=$4,data=jsonb_set(data,'{updated_at}',to_jsonb($5::text),true) \
         WHERE tenant_id=$1 AND source_id=$2 AND id=$3",
    )
    .bind(tenant_id).bind(source_id).bind(thread_id).bind(timestamp(&now)?).bind(&now)
    .execute(db).await.map_err(|error| error.to_string())?;
    Ok(())
}

pub async fn try_acquire_summary_slot(
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    thread_id: &str,
    job_run_id: &str,
) -> Result<Option<EngineThread>, String> {
    let now = now_rfc3339();
    let expires = now_plus_seconds_rfc3339(SUMMARY_LOCK_TIMEOUT_SECS);
    conditional_summary_mutation(db, tenant_id, source_id, thread_id,
        "AND (summary_status<>'running' OR summary_lock_expires_at IS NULL OR summary_lock_expires_at<=now())",
        None,
        |thread| {
            thread.summary_status = "running".to_string();
            thread.summary_job_run_id = Some(job_run_id.to_string());
            thread.summary_locked_at = Some(now.clone());
            thread.summary_lock_expires_at = Some(expires.clone());
        }).await
}

pub async fn refresh_summary_slot(
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    thread_id: &str,
    job_run_id: &str,
    lock_timeout_secs: i64,
) -> Result<bool, String> {
    let expires = now_plus_seconds_rfc3339(lock_timeout_secs.max(SUMMARY_LOCK_TIMEOUT_SECS));
    Ok(conditional_summary_mutation(
        db,
        tenant_id,
        source_id,
        thread_id,
        "AND summary_status='running' AND summary_job_run_id=$4",
        Some(job_run_id),
        |thread| {
            thread.summary_lock_expires_at = Some(expires.clone());
        },
    )
    .await?
    .is_some())
}

pub async fn release_summary_slot(
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    thread_id: &str,
    job_run_id: &str,
    consumed_record_count: i64,
    consumed_summary_tokens: i64,
) -> Result<Option<EngineThread>, String> {
    conditional_summary_mutation(
        db,
        tenant_id,
        source_id,
        thread_id,
        "AND summary_job_run_id=$4",
        Some(job_run_id),
        |thread| {
            thread.pending_record_count =
                (thread.pending_record_count - consumed_record_count.max(0)).max(0);
            thread.pending_summary_tokens =
                (thread.pending_summary_tokens - consumed_summary_tokens.max(0)).max(0);
            thread.summary_status = if thread.pending_record_count > 0 {
                "pending"
            } else {
                "idle"
            }
            .to_string();
            thread.summary_job_run_id = None;
            thread.summary_locked_at = None;
            thread.summary_lock_expires_at = None;
        },
    )
    .await
}

pub async fn try_acquire_rollup_slot(
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    thread_id: &str,
    job_run_id: &str,
    lock_timeout_secs: i64,
) -> Result<Option<EngineThread>, String> {
    let now = now_rfc3339();
    let expires = now_plus_seconds_rfc3339(lock_timeout_secs.max(30));
    let row = sqlx::query_scalar::<_, Json<serde_json::Value>>(
        "UPDATE engine_threads SET rollup_job_run_id=$4,rollup_locked_at=$5,rollup_lock_expires_at=$6, \
         updated_at=$5,data=jsonb_set(data,'{updated_at}',to_jsonb($7::text),true) \
         WHERE tenant_id=$1 AND source_id=$2 AND id=$3 \
         AND (rollup_job_run_id IS NULL OR rollup_lock_expires_at IS NULL OR rollup_lock_expires_at<=now()) \
         RETURNING data",
    ).bind(tenant_id).bind(source_id).bind(thread_id).bind(job_run_id)
      .bind(timestamp(&now)?).bind(timestamp(&expires)?).bind(&now)
      .fetch_optional(db).await.map_err(|error| error.to_string())?;
    row.map(decode).transpose()
}

pub async fn release_rollup_slot(
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    thread_id: &str,
    job_run_id: &str,
) -> Result<(), String> {
    let now = now_rfc3339();
    sqlx::query(
        "UPDATE engine_threads SET rollup_job_run_id=NULL,rollup_locked_at=NULL,rollup_lock_expires_at=NULL, \
         updated_at=$5,data=jsonb_set(data,'{updated_at}',to_jsonb($6::text),true) \
         WHERE tenant_id=$1 AND source_id=$2 AND id=$3 AND rollup_job_run_id=$4",
    ).bind(tenant_id).bind(source_id).bind(thread_id).bind(job_run_id)
      .bind(timestamp(&now)?).bind(&now).execute(db).await.map_err(|error| error.to_string())?;
    Ok(())
}

async fn conditional_summary_mutation<F>(
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    thread_id: &str,
    condition: &str,
    condition_value: Option<&str>,
    mutate: F,
) -> Result<Option<EngineThread>, String>
where
    F: FnOnce(&mut EngineThread),
{
    let mut tx = db.begin().await.map_err(|error| error.to_string())?;
    let sql = format!("SELECT data FROM engine_threads WHERE tenant_id=$1 AND source_id=$2 AND id=$3 {condition} FOR UPDATE");
    let mut query = sqlx::query_scalar::<_, Json<serde_json::Value>>(&sql)
        .bind(tenant_id)
        .bind(source_id)
        .bind(thread_id);
    if let Some(value) = condition_value {
        query = query.bind(value);
    }
    let row = query
        .fetch_optional(&mut *tx)
        .await
        .map_err(|error| error.to_string())?;
    let Some(row) = row else {
        tx.commit().await.map_err(|error| error.to_string())?;
        return Ok(None);
    };
    let mut thread: EngineThread = decode(row)?;
    mutate(&mut thread);
    thread.updated_at = now_rfc3339();
    save_thread(&mut tx, &thread).await?;
    tx.commit().await.map_err(|error| error.to_string())?;
    Ok(Some(thread))
}

async fn mutate_thread<F>(
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    thread_id: &str,
    allow_running: bool,
    mutate: F,
) -> Result<Option<EngineThread>, String>
where
    F: FnOnce(&mut EngineThread) -> bool,
{
    let condition = if allow_running {
        ""
    } else {
        "AND summary_status<>'running'"
    };
    let mut tx = db.begin().await.map_err(|error| error.to_string())?;
    let sql = format!("SELECT data FROM engine_threads WHERE tenant_id=$1 AND source_id=$2 AND id=$3 {condition} FOR UPDATE");
    let row = sqlx::query_scalar::<_, Json<serde_json::Value>>(&sql)
        .bind(tenant_id)
        .bind(source_id)
        .bind(thread_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|error| error.to_string())?;
    let Some(row) = row else {
        tx.commit().await.map_err(|error| error.to_string())?;
        return Ok(None);
    };
    let mut thread: EngineThread = decode(row)?;
    if !mutate(&mut thread) {
        tx.commit().await.map_err(|error| error.to_string())?;
        return Ok(None);
    }
    thread.updated_at = now_rfc3339();
    save_thread(&mut tx, &thread).await?;
    tx.commit().await.map_err(|error| error.to_string())?;
    Ok(Some(thread))
}

async fn save_thread(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    thread: &EngineThread,
) -> Result<(), String> {
    sqlx::query(
        "UPDATE engine_threads SET summary_status=$2,pending_record_count=$3,pending_summary_tokens=$4, \
         summary_job_run_id=$5,summary_locked_at=$6,summary_lock_expires_at=$7,updated_at=$8,data=$9 WHERE id=$1",
    ).bind(&thread.id).bind(&thread.summary_status).bind(thread.pending_record_count)
      .bind(thread.pending_summary_tokens).bind(&thread.summary_job_run_id)
      .bind(optional_timestamp(thread.summary_locked_at.as_deref())?)
      .bind(optional_timestamp(thread.summary_lock_expires_at.as_deref())?)
      .bind(timestamp(&thread.updated_at)?).bind(json(thread)?)
      .execute(&mut **tx).await.map_err(|error| error.to_string())?;
    Ok(())
}

async fn load_thread(
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    thread_id: &str,
) -> Result<Option<EngineThread>, String> {
    sqlx::query_scalar::<_, Json<serde_json::Value>>(
        "SELECT data FROM engine_threads WHERE tenant_id=$1 AND source_id=$2 AND id=$3",
    )
    .bind(tenant_id)
    .bind(source_id)
    .bind(thread_id)
    .fetch_optional(db)
    .await
    .map_err(|error| error.to_string())?
    .map(decode)
    .transpose()
}

async fn count_scope(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    table: &str,
    tenant_id: &str,
    source_id: &str,
    thread_id: &str,
) -> Result<i64, String> {
    let sql = format!(
        "SELECT count(*) FROM {table} WHERE tenant_id=$1 AND source_id=$2 AND thread_id=$3"
    );
    sqlx::query_scalar(&sql)
        .bind(tenant_id)
        .bind(source_id)
        .bind(thread_id)
        .fetch_one(&mut **tx)
        .await
        .map_err(|error| error.to_string())
}
