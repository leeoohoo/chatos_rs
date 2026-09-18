// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use sqlx::types::Json;

use crate::db::Db;
use crate::models::{now_rfc3339, EngineRecord};
use crate::repositories::postgres::{decode, json, timestamp};

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

pub async fn reset_records_summary_by_summary_id(
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    thread_id: &str,
    summary_id: &str,
) -> Result<usize, String> {
    let summary_id = summary_id.trim();
    if summary_id.is_empty() {
        return Ok(0);
    }
    update_records(
        db,
        tenant_id,
        source_id,
        thread_id,
        "summary_id=$4",
        None,
        None,
        Some(summary_id),
        |record, _| {
            record.summary_status = "pending".to_string();
            record.summary_id = None;
            record.summarized_at = None;
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
    let mut query = sqlx::query_scalar::<_, Json<serde_json::Value>>(&sql)
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
            "UPDATE engine_records SET summary_status=$2,summary_id=$3,summary_job_run_id=$4,summary_started_at=$5,data=$6 WHERE id=$1"
        ).bind(&record.id).bind(&record.summary_status).bind(&record.summary_id).bind(claim_job).bind(started).bind(json(&record)?)
          .execute(&mut *tx).await.map_err(|e|e.to_string())?.rows_affected() as usize;
    }
    tx.commit().await.map_err(|e| e.to_string())?;
    Ok(count)
}
