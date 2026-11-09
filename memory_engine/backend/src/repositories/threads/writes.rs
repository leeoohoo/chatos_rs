// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use sqlx::types::Json;

use crate::db::Db;
use crate::models::{
    now_plus_seconds_rfc3339, now_rfc3339, DeleteThreadResponse, EngineThread, UpsertThreadRequest,
};
use crate::repositories::postgres::{decode, json, optional_timestamp, timestamp};
use crate::repositories::subject_memory_scopes;

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
    let identity_scopes = subject_memory_scopes::thread_identity_scope_requests(&thread)?;
    if let Some(existing) = existing.as_ref() {
        let existing_scopes = subject_memory_scopes::thread_identity_scope_requests(existing)?;
        if !existing_scopes.is_empty()
            && identity_scopes
                .iter()
                .map(|(_, request)| request.subject_id.as_str())
                .collect::<Vec<_>>()
                != existing_scopes
                    .iter()
                    .map(|(_, request)| request.subject_id.as_str())
                    .collect::<Vec<_>>()
        {
            return Err("memory-schema:v2 thread identity is immutable".to_string());
        }
    }
    let result = sqlx::query(
        "INSERT INTO engine_threads \
         (id,tenant_id,source_id,subject_id,thread_type,external_thread_id,status, \
          created_at,updated_at,archived_at,data) \
         VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11) \
         ON CONFLICT(tenant_id,source_id,id) DO UPDATE SET subject_id=EXCLUDED.subject_id,thread_type=EXCLUDED.thread_type, \
         external_thread_id=EXCLUDED.external_thread_id,status=EXCLUDED.status, \
         updated_at=EXCLUDED.updated_at,archived_at=EXCLUDED.archived_at,data=EXCLUDED.data \
         WHERE engine_threads.recall_eligible \
         ",
    )
    .bind(&thread.id)
    .bind(&thread.tenant_id)
    .bind(&thread.source_id)
    .bind(&thread.subject_id)
    .bind(&thread.thread_type)
    .bind(&thread.external_thread_id)
    .bind(&thread.status)
    .bind(timestamp(&thread.created_at)?)
    .bind(timestamp(&thread.updated_at)?)
    .bind(optional_timestamp(thread.archived_at.as_deref())?)
    .bind(json(&thread)?)
    .execute(db)
    .await
    .map_err(|error| error.to_string())?;
    if result.rows_affected() == 0 {
        return Err("retired thread identity cannot be reused".to_string());
    }
    subject_memory_scopes::ensure_thread_identity_scopes(db, &thread).await?;
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
        count_recall_eligible(&mut tx, "engine_records", tenant_id, source_id, thread_id).await?;
    let deleted_summaries =
        count_recall_eligible(&mut tx, "engine_summaries", tenant_id, source_id, thread_id).await?;
    let deleted_snapshots = 0;
    let now = now_rfc3339();
    sqlx::query(
        "UPDATE engine_summary_record_coverage SET status='retired',retired_at=$4, \
         retired_reason='thread_retired' WHERE tenant_id=$1 AND source_id=$2 AND thread_id=$3 \
         AND status='active'",
    )
    .bind(tenant_id)
    .bind(source_id)
    .bind(thread_id)
    .bind(timestamp(&now)?)
    .execute(&mut *tx)
    .await
    .map_err(|error| error.to_string())?;
    sqlx::query(
        "UPDATE engine_memory_derivation_edges edge SET status='retired',retired_at=$4, \
         retired_reason='thread_retired' WHERE edge.tenant_id=$1 AND edge.source_id=$2 \
         AND edge.status='active' AND ( \
           (edge.input_kind='thread_summary' AND EXISTS(SELECT 1 FROM engine_summaries summary \
             WHERE summary.tenant_id=$1 AND summary.source_id=$2 AND summary.id=edge.input_id \
               AND summary.thread_id=$3)) OR \
           (edge.output_kind='thread_summary' AND EXISTS(SELECT 1 FROM engine_summaries summary \
             WHERE summary.tenant_id=$1 AND summary.source_id=$2 AND summary.id=edge.output_id \
               AND summary.thread_id=$3)))",
    )
    .bind(tenant_id)
    .bind(source_id)
    .bind(thread_id)
    .bind(timestamp(&now)?)
    .execute(&mut *tx)
    .await
    .map_err(|error| error.to_string())?;
    sqlx::query(
        "UPDATE engine_memory_fact_projections fact SET status='retired',retired_at=$4 \
         WHERE fact.tenant_id=$1 AND fact.source_id=$2 AND fact.output_kind='thread_summary' \
         AND fact.status IN ('active','uncertain') AND EXISTS( \
           SELECT 1 FROM engine_summaries summary WHERE summary.tenant_id=$1 \
           AND summary.source_id=$2 AND summary.thread_id=$3 AND summary.id=fact.output_id)",
    )
    .bind(tenant_id)
    .bind(source_id)
    .bind(thread_id)
    .bind(timestamp(&now)?)
    .execute(&mut *tx)
    .await
    .map_err(|error| error.to_string())?;
    sqlx::query(
        "UPDATE engine_records SET recall_eligible=false,retired_at=$4, \
         retired_reason='thread_retired',summary_status='retired', \
         data=jsonb_set(data,'{summary_status}',to_jsonb('retired'::text),true) \
         WHERE tenant_id=$1 AND source_id=$2 AND thread_id=$3 AND recall_eligible",
    )
    .bind(tenant_id)
    .bind(source_id)
    .bind(thread_id)
    .bind(timestamp(&now)?)
    .execute(&mut *tx)
    .await
    .map_err(|error| error.to_string())?;
    sqlx::query(
        "UPDATE engine_summaries SET status='retired',recall_eligible=false,retired_at=$4, \
         retired_reason='thread_retired',updated_at=$4, \
         data=jsonb_set(jsonb_set(data,'{status}',to_jsonb('retired'::text),true), \
                        '{updated_at}',to_jsonb($5::text),true) \
         WHERE tenant_id=$1 AND source_id=$2 AND thread_id=$3 AND recall_eligible",
    )
    .bind(tenant_id)
    .bind(source_id)
    .bind(thread_id)
    .bind(timestamp(&now)?)
    .bind(&now)
    .execute(&mut *tx)
    .await
    .map_err(|error| error.to_string())?;
    sqlx::query(
        "UPDATE engine_thread_summary_partitions SET pending_record_count=0, \
         pending_summary_tokens=0,summary_status='idle',summary_job_run_id=NULL, \
         summary_locked_at=NULL,summary_lock_expires_at=NULL,updated_at=$4 \
         WHERE tenant_id=$1 AND source_id=$2 AND thread_id=$3",
    )
    .bind(tenant_id)
    .bind(source_id)
    .bind(thread_id)
    .bind(timestamp(&now)?)
    .execute(&mut *tx)
    .await
    .map_err(|error| error.to_string())?;
    sqlx::query(
        "DELETE FROM engine_compact_turns WHERE tenant_id=$1 AND source_id=$2 AND thread_id=$3",
    )
    .bind(tenant_id)
    .bind(source_id)
    .bind(thread_id)
    .execute(&mut *tx)
    .await
    .map_err(|error| error.to_string())?;
    let deleted_thread = sqlx::query(
        "UPDATE engine_threads SET status='retired',recall_eligible=false,retired_at=$4, \
         retired_reason='business_delete',updated_at=$4, \
         data=jsonb_set(jsonb_set(data,'{status}',to_jsonb('retired'::text),true), \
                        '{updated_at}',to_jsonb($5::text),true) \
         WHERE tenant_id=$1 AND source_id=$2 AND id=$3 AND recall_eligible",
    )
    .bind(tenant_id)
    .bind(source_id)
    .bind(thread_id)
    .bind(timestamp(&now)?)
    .bind(&now)
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
         WHERE tenant_id=$1 AND source_id=$2 AND id=$3 AND recall_eligible",
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
         WHERE tenant_id=$1 AND source_id=$2 AND id=$3 AND recall_eligible",
    )
    .bind(tenant_id).bind(source_id).bind(thread_id).bind(timestamp(&now)?).bind(&now)
    .execute(db).await.map_err(|error| error.to_string())?;
    Ok(())
}

async fn load_thread(
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    thread_id: &str,
) -> Result<Option<EngineThread>, String> {
    sqlx::query_scalar::<_, Json<serde_json::Value>>(
        "SELECT data FROM engine_threads WHERE tenant_id=$1 AND source_id=$2 AND id=$3 \
         AND recall_eligible",
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

async fn count_recall_eligible(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    table: &str,
    tenant_id: &str,
    source_id: &str,
    thread_id: &str,
) -> Result<i64, String> {
    let sql = format!(
        "SELECT count(*) FROM {table} WHERE tenant_id=$1 AND source_id=$2 AND thread_id=$3 \
         AND recall_eligible"
    );
    sqlx::query_scalar(sqlx::AssertSqlSafe(sql))
        .bind(tenant_id)
        .bind(source_id)
        .bind(thread_id)
        .fetch_one(&mut **tx)
        .await
        .map_err(|error| error.to_string())
}
