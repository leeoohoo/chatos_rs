// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use sqlx::types::Json;
use sqlx::{Postgres, Transaction};

use crate::db::Db;
use crate::models::{now_rfc3339, EngineRecord};
use crate::repositories::postgres::{decode, json, timestamp};

#[derive(Debug, Clone)]
pub struct RestorePendingResult {
    pub changed: bool,
    pub thread_id: String,
    pub project_partition_key: String,
    pub previous_status: String,
}

pub async fn restore_record_pending(
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    record_id: &str,
) -> Result<RestorePendingResult, String> {
    let mut tx = db.begin().await.map_err(|error| error.to_string())?;
    let restored = restore_record_pending_in_tx(&mut tx, tenant_id, source_id, record_id).await?;
    tx.commit().await.map_err(|error| error.to_string())?;
    Ok(restored)
}

pub async fn restore_record_pending_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: &str,
    source_id: &str,
    record_id: &str,
) -> Result<RestorePendingResult, String> {
    let row = sqlx::query_as::<_, (Json<serde_json::Value>, String, String, String, bool, Option<String>)>(
        "SELECT data,thread_id,project_partition_key,summary_status,recall_eligible,summary_job_run_id \
         FROM engine_records WHERE tenant_id=$1 AND source_id=$2 AND id=$3 FOR UPDATE",
    )
    .bind(tenant_id)
    .bind(source_id)
    .bind(record_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(|error| error.to_string())?
    .ok_or_else(|| "record not found".to_string())?;
    if !row.4 {
        return Err("retired records cannot be restored by the pending repair operation".to_string());
    }
    let active_coverage = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM engine_summary_record_coverage WHERE tenant_id=$1 \
         AND source_id=$2 AND record_id=$3 AND status='active')",
    )
    .bind(tenant_id)
    .bind(source_id)
    .bind(record_id)
    .fetch_one(&mut **tx)
    .await
    .map_err(|error| error.to_string())?;
    if active_coverage {
        return Err("record still has active coverage; retire or resummarize its complete range"
            .to_string());
    }
    if row.3 == "pending" {
        return Ok(RestorePendingResult {
            changed: false,
            thread_id: row.1,
            project_partition_key: row.2,
            previous_status: row.3,
        });
    }
    let mut record: EngineRecord = decode(row.0)?;
    let pending_tokens = crate::repositories::records::estimate_pending_record_tokens(&record);
    record.summary_status = "pending".to_string();
    record.summary_id = None;
    record.summarized_at = None;
    let updated = sqlx::query(
        "UPDATE engine_records SET summary_status='pending',summary_id=NULL,summary_job_run_id=NULL, \
         summary_started_at=NULL,data=$4 WHERE tenant_id=$1 AND source_id=$2 AND id=$3 \
           AND recall_eligible",
    )
    .bind(tenant_id)
    .bind(source_id)
    .bind(record_id)
    .bind(json(&record)?)
    .execute(&mut **tx)
    .await
    .map_err(|error| error.to_string())?;
    if updated.rows_affected() != 1 {
        return Err("record changed before pending restore".to_string());
    }
    crate::repositories::thread_partitions::lock_or_create(
        tx, tenant_id, source_id, &row.1, &row.2,
    )
    .await?;
    if row.3 == "summarizing" {
        if let Some(job_run_id) = row.5.as_deref() {
            sqlx::query(
                "UPDATE engine_thread_summary_partitions SET summary_status='pending', \
                 summary_job_run_id=NULL,summary_locked_at=NULL,summary_lock_expires_at=NULL, \
                 updated_at=now() WHERE tenant_id=$1 AND source_id=$2 AND thread_id=$3 \
                   AND project_partition_key=$4 AND summary_job_run_id=$5",
            )
            .bind(tenant_id).bind(source_id).bind(&row.1).bind(&row.2).bind(job_run_id)
            .execute(&mut **tx).await.map_err(|error| error.to_string())?;
        }
    } else {
        let version = crate::repositories::thread_partitions::apply_ingest_delta(
            tx, tenant_id, source_id, &row.1, &row.2, 1, pending_tokens, true,
        ).await?;
        if let Some(version) = version {
            crate::repositories::derived_events::enqueue_thread_summary_request(
                tx, tenant_id, source_id, &row.1, &row.2, version,
            ).await?;
        }
    }
    Ok(RestorePendingResult {
        changed: true,
        thread_id: row.1,
        project_partition_key: row.2,
        previous_status: row.3,
    })
}

pub async fn claim_records_for_summary(
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    thread_id: &str,
    record_ids: &[String],
    job_run_id: &str,
) -> Result<usize, String> {
    if record_ids.is_empty() {
        return Ok(0);
    }
    update_records(
        db,
        tenant_id,
        source_id,
        thread_id,
        "id=ANY($4) AND summary_status IN ('pending','') AND $5::text IS NOT NULL",
        Some(record_ids),
        Some(job_run_id),
        None,
        |record, _| {
            record.summary_status = "summarizing".to_string();
            record.summary_id = None;
            record.summarized_at = None;
        },
    )
    .await
}

pub async fn release_records_from_summary(
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    thread_id: &str,
    job_run_id: &str,
) -> Result<usize, String> {
    update_records(
        db,
        tenant_id,
        source_id,
        thread_id,
        "summary_status='summarizing' AND summary_job_run_id=$4",
        None,
        Some(job_run_id),
        None,
        |record, _| record.summary_status = "pending".to_string(),
    )
    .await
}

pub async fn mark_claimed_records_summarized(
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    thread_id: &str,
    record_ids: &[String],
    job_run_id: &str,
    summary_id: &str,
) -> Result<usize, String> {
    if record_ids.is_empty() {
        return Ok(0);
    }
    update_records(
        db,
        tenant_id,
        source_id,
        thread_id,
        "id=ANY($4) AND summary_status='summarizing' AND summary_job_run_id=$5",
        Some(record_ids),
        Some(job_run_id),
        Some(summary_id),
        |record, now| {
            record.summary_status = "summarized".to_string();
            record.summary_id = Some(summary_id.to_string());
            record.summarized_at = Some(now.to_string());
        },
    )
    .await
}

pub async fn mark_records_summarized(
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    thread_id: &str,
    record_ids: &[String],
    summary_id: &str,
) -> Result<usize, String> {
    if record_ids.is_empty() {
        return Ok(0);
    }
    update_records(
        db,
        tenant_id,
        source_id,
        thread_id,
        "id=ANY($4)",
        Some(record_ids),
        None,
        Some(summary_id),
        |record, now| {
            record.summary_status = "summarized".to_string();
            record.summary_id = Some(summary_id.to_string());
            record.summarized_at = Some(now.to_string());
        },
    )
    .await
}

#[allow(clippy::too_many_arguments)]
async fn update_records<F>(
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    thread_id: &str,
    condition: &str,
    ids: Option<&[String]>,
    job_run_id: Option<&str>,
    summary_id: Option<&str>,
    mutate: F,
) -> Result<usize, String>
where
    F: Fn(&mut EngineRecord, &str),
{
    let mut tx = db.begin().await.map_err(|e| e.to_string())?;
    let sql=format!("SELECT data FROM engine_records WHERE tenant_id=$1 AND source_id=$2 AND thread_id=$3 AND {condition} FOR UPDATE");
    let mut query = sqlx::query_scalar::<_, Json<serde_json::Value>>(sqlx::AssertSqlSafe(sql))
        .bind(tenant_id)
        .bind(source_id)
        .bind(thread_id);
    if let Some(ids) = ids {
        query = query.bind(ids);
    }
    if let Some(job) = job_run_id {
        query = query.bind(job);
    }
    if ids.is_none() && job_run_id.is_none() {
        if let Some(summary) = summary_id {
            query = query.bind(summary);
        }
    }
    let rows = query.fetch_all(&mut *tx).await.map_err(|e| e.to_string())?;
    let now = now_rfc3339();
    let mut count = 0;
    for row in rows {
        let mut record: EngineRecord = decode(row)?;
        mutate(&mut record, &now);
        let (claim_job, started) = if record.summary_status == "summarizing" {
            (job_run_id, Some(timestamp(&now)?))
        } else {
            (None, None)
        };
        count+=sqlx::query(
            "UPDATE engine_records SET summary_status=$4,summary_id=$5,summary_job_run_id=$6,summary_started_at=$7,data=$8 \
             WHERE tenant_id=$1 AND source_id=$2 AND id=$3"
        ).bind(&record.tenant_id).bind(&record.source_id).bind(&record.id)
          .bind(&record.summary_status).bind(&record.summary_id).bind(claim_job).bind(started).bind(json(&record)?)
          .execute(&mut *tx).await.map_err(|e|e.to_string())?.rows_affected() as usize;
    }
    tx.commit().await.map_err(|e| e.to_string())?;
    Ok(count)
}
