// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::common::new_summary;
use crate::db::Db;
use crate::models::{now_rfc3339, EngineSummary, UpsertThreadSummaryRequest};
use crate::repositories::postgres::{decode, json, timestamp};
use sqlx::types::Json;

pub async fn delete_thread_summary(
    db: &Db,
    thread_id: &str,
    summary_id: &str,
    tenant_id: Option<&str>,
    source_id: Option<&str>,
) -> Result<usize, String> {
    let tenant = tenant_id.map(str::trim).filter(|v| !v.is_empty());
    let source = source_id.map(str::trim).filter(|v| !v.is_empty());
    let reset = if let (Some(t), Some(s)) = (tenant, source) {
        crate::repositories::records::reset_records_summary_by_summary_id(
            db, t, s, thread_id, summary_id,
        )
        .await?
    } else {
        0
    };
    let mut sql = "DELETE FROM engine_summaries WHERE thread_id=$1 AND id=$2".to_string();
    if tenant.is_some() {
        sql.push_str(" AND tenant_id=$3");
    }
    if source.is_some() {
        sql.push_str(if tenant.is_some() {
            " AND source_id=$4"
        } else {
            " AND source_id=$3"
        });
    }
    let mut q = sqlx::query(&sql).bind(thread_id).bind(summary_id);
    if let Some(v) = tenant {
        q = q.bind(v);
    }
    if let Some(v) = source {
        q = q.bind(v);
    }
    let deleted = q
        .execute(db)
        .await
        .map_err(|e| e.to_string())?
        .rows_affected();
    if reset > 0 {
        if let (Some(t), Some(s)) = (tenant, source) {
            crate::repositories::threads::refresh_summary_queue_state(db, t, s, thread_id).await?;
        }
    }
    Ok(if deleted > 0 || reset > 0 { reset } else { 0 })
}

pub async fn create_thread_summary(
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    thread_id: &str,
    subject_id: &str,
    summary_text: &str,
    source_record_start_id: Option<String>,
    source_record_end_id: Option<String>,
    source_record_count: usize,
) -> Result<EngineSummary, String> {
    create_thread_summary_with_type(
        db,
        tenant_id,
        source_id,
        thread_id,
        subject_id,
        "thread_incremental",
        None,
        summary_text,
        source_record_start_id,
        source_record_end_id,
        source_record_count,
        Some(serde_json::json!({"generator":"memory_engine_summary_v1"})),
    )
    .await
}

pub async fn upsert_thread_summary(
    db: &Db,
    thread_id: &str,
    summary_id: &str,
    req: UpsertThreadSummaryRequest,
) -> Result<EngineSummary, String> {
    let existing = sqlx::query_scalar::<_, Json<serde_json::Value>>(
        "SELECT data FROM engine_summaries WHERE thread_id=$1 AND id=$2",
    )
    .bind(thread_id)
    .bind(summary_id)
    .fetch_optional(db)
    .await
    .map_err(|e| e.to_string())?
    .map(decode::<EngineSummary>)
    .transpose()?;
    let now = now_rfc3339();
    let summary = EngineSummary {
        id: summary_id.to_string(),
        tenant_id: req.tenant_id,
        source_id: req.source_id,
        thread_id: thread_id.to_string(),
        subject_id: req.subject_id,
        summary_type: req.summary_type,
        level: req.level.unwrap_or(0).max(0),
        source_digest: req.source_digest,
        summary_text: req.summary_text,
        source_record_start_id: req.source_record_start_id,
        source_record_end_id: req.source_record_end_id,
        source_record_count: req.source_record_count.unwrap_or(0).max(0),
        status: req.status.unwrap_or_else(|| "done".to_string()),
        rollup_status: req.rollup_status.unwrap_or_else(|| "pending".to_string()),
        rollup_summary_id: req.rollup_summary_id,
        rolled_up_at: req.rolled_up_at,
        subject_memory_summarized: req.subject_memory_summarized.unwrap_or(0).max(0),
        subject_memory_summarized_at: req.subject_memory_summarized_at,
        metadata: req.metadata,
        created_at: existing
            .map(|v| v.created_at)
            .or(req.created_at)
            .unwrap_or_else(|| now.clone()),
        updated_at: req.updated_at.unwrap_or_else(|| now.clone()),
    };
    upsert_summary_row(db, &summary, true).await?;
    Ok(summary)
}

#[allow(clippy::too_many_arguments)]
pub async fn create_thread_summary_with_type(
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    thread_id: &str,
    subject_id: &str,
    summary_type: &str,
    source_digest: Option<String>,
    summary_text: &str,
    source_record_start_id: Option<String>,
    source_record_end_id: Option<String>,
    source_record_count: usize,
    metadata: Option<serde_json::Value>,
) -> Result<EngineSummary, String> {
    let summary = new_summary(
        tenant_id,
        source_id,
        thread_id,
        subject_id,
        summary_type,
        0,
        source_digest,
        summary_text,
        source_record_start_id,
        source_record_end_id,
        source_record_count,
        metadata,
    );
    insert_summary(db, &summary).await?;
    Ok(summary)
}

#[allow(clippy::too_many_arguments)]
pub async fn create_rollup_summary(
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    thread_id: &str,
    subject_id: &str,
    level: i64,
    source_digest: Option<String>,
    summary_text: &str,
    source_record_start_id: Option<String>,
    source_record_end_id: Option<String>,
    source_record_count: usize,
    metadata: Option<serde_json::Value>,
) -> Result<EngineSummary, String> {
    let summary = new_summary(
        tenant_id,
        source_id,
        thread_id,
        subject_id,
        "thread_incremental",
        level,
        source_digest,
        summary_text,
        source_record_start_id,
        source_record_end_id,
        source_record_count,
        metadata,
    );
    insert_summary(db, &summary).await?;
    Ok(summary)
}

async fn insert_summary(db: &Db, summary: &EngineSummary) -> Result<(), String> {
    sqlx::query("INSERT INTO engine_summaries(id,tenant_id,source_id,thread_id,subject_id,summary_type,level,source_digest,status,rollup_status,subject_memory_summarized,rollup_dispatch_pending,rollup_dispatch_version,rollup_dispatch_requested_at,subject_memory_source_dispatch_pending,subject_memory_source_dispatch_version,subject_memory_source_dispatch_requested_at,created_at,updated_at,data) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,true,1,$12,true,1,$12,$12,$13,$14)")
        .bind(&summary.id).bind(&summary.tenant_id).bind(&summary.source_id).bind(&summary.thread_id).bind(&summary.subject_id).bind(&summary.summary_type)
        .bind(summary.level).bind(&summary.source_digest).bind(&summary.status).bind(&summary.rollup_status).bind(summary.subject_memory_summarized)
        .bind(timestamp(&summary.created_at)?).bind(timestamp(&summary.updated_at)?).bind(json(summary)?)
        .execute(db).await.map(|_|()).map_err(|e|e.to_string())
}

async fn upsert_summary_row(
    db: &Db,
    summary: &EngineSummary,
    increment_dispatch: bool,
) -> Result<(), String> {
    let rollup = summary.status == "done" && summary.rollup_status == "pending";
    let subject = summary.status == "done" && summary.subject_memory_summarized == 0;
    sqlx::query("INSERT INTO engine_summaries(id,tenant_id,source_id,thread_id,subject_id,summary_type,level,source_digest,status,rollup_status,subject_memory_summarized,rollup_dispatch_pending,rollup_dispatch_version,rollup_dispatch_requested_at,subject_memory_source_dispatch_pending,subject_memory_source_dispatch_version,subject_memory_source_dispatch_requested_at,created_at,updated_at,data) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,CASE WHEN $12 THEN 1 ELSE 0 END,CASE WHEN $12 THEN now() ELSE NULL END,$13,CASE WHEN $13 THEN 1 ELSE 0 END,CASE WHEN $13 THEN now() ELSE NULL END,$14,$15,$16) ON CONFLICT(id) DO UPDATE SET tenant_id=EXCLUDED.tenant_id,source_id=EXCLUDED.source_id,thread_id=EXCLUDED.thread_id,subject_id=EXCLUDED.subject_id,summary_type=EXCLUDED.summary_type,level=EXCLUDED.level,source_digest=EXCLUDED.source_digest,status=EXCLUDED.status,rollup_status=EXCLUDED.rollup_status,subject_memory_summarized=EXCLUDED.subject_memory_summarized,rollup_dispatch_pending=$12,rollup_dispatch_version=engine_summaries.rollup_dispatch_version+CASE WHEN $12 THEN 1 ELSE 0 END,rollup_dispatch_requested_at=CASE WHEN $12 THEN now() ELSE engine_summaries.rollup_dispatch_requested_at END,rollup_dispatch_last_error=CASE WHEN $12 THEN NULL ELSE engine_summaries.rollup_dispatch_last_error END,subject_memory_source_dispatch_pending=$13,subject_memory_source_dispatch_version=engine_summaries.subject_memory_source_dispatch_version+CASE WHEN $13 THEN 1 ELSE 0 END,subject_memory_source_dispatch_requested_at=CASE WHEN $13 THEN now() ELSE engine_summaries.subject_memory_source_dispatch_requested_at END,subject_memory_source_dispatch_last_error=CASE WHEN $13 THEN NULL ELSE engine_summaries.subject_memory_source_dispatch_last_error END,updated_at=EXCLUDED.updated_at,data=EXCLUDED.data")
        .bind(&summary.id).bind(&summary.tenant_id).bind(&summary.source_id).bind(&summary.thread_id).bind(&summary.subject_id).bind(&summary.summary_type)
        .bind(summary.level).bind(&summary.source_digest).bind(&summary.status).bind(&summary.rollup_status).bind(summary.subject_memory_summarized)
        .bind(rollup&&increment_dispatch).bind(subject&&increment_dispatch).bind(timestamp(&summary.created_at)?).bind(timestamp(&summary.updated_at)?).bind(json(summary)?)
        .execute(db).await.map(|_|()).map_err(|e|e.to_string())
}
