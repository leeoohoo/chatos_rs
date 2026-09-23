// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use sqlx::types::Json;
use tracing::{info, warn};
use uuid::Uuid;

use crate::db::Db;
use crate::models::{
    now_rfc3339, CreateEngineJobRunRequest, EngineJobRun, FinishEngineJobRunRequest,
};
use crate::repositories::postgres::{decode, json, optional_timestamp, timestamp};

pub async fn create_job_run(
    db: &Db,
    req: CreateEngineJobRunRequest,
) -> Result<EngineJobRun, String> {
    let job_run = EngineJobRun {
        id: Uuid::new_v4().to_string(),
        job_type: req.job_type,
        trigger_type: req.trigger_type,
        tenant_id: req.tenant_id,
        source_id: req.source_id,
        thread_id: req.thread_id,
        subject_id: req.subject_id,
        thread_label: req.thread_label,
        thread_display_name: None,
        status: "running".to_string(),
        input_count: 0,
        output_count: 0,
        processed_count: 0,
        success_count: 0,
        error_count: 0,
        metadata: req.metadata,
        error_message: None,
        started_at: now_rfc3339(),
        finished_at: None,
    };
    sqlx::query(
        "INSERT INTO engine_job_runs \
         (id,job_type,trigger_type,tenant_id,source_id,thread_id,subject_id,thread_label,status, \
          started_at,finished_at,data) \
         VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,NULL,$11)",
    )
    .bind(&job_run.id)
    .bind(&job_run.job_type)
    .bind(&job_run.trigger_type)
    .bind(&job_run.tenant_id)
    .bind(&job_run.source_id)
    .bind(&job_run.thread_id)
    .bind(&job_run.subject_id)
    .bind(&job_run.thread_label)
    .bind(&job_run.status)
    .bind(timestamp(&job_run.started_at)?)
    .bind(json(&job_run)?)
    .execute(db)
    .await
    .map_err(|error| error.to_string())?;
    info!(job_run_id = %job_run.id, job_type = %job_run.job_type, "created Memory Engine job run");
    Ok(job_run)
}

pub async fn finish_job_run(
    db: &Db,
    id: &str,
    req: FinishEngineJobRunRequest,
) -> Result<Option<EngineJobRun>, String> {
    let mut tx = db.begin().await.map_err(|error| error.to_string())?;
    let data = sqlx::query_scalar::<_, Json<serde_json::Value>>(
        "SELECT data FROM engine_job_runs WHERE id=$1 FOR UPDATE",
    )
    .bind(id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|error| error.to_string())?;
    let Some(data) = data else {
        tx.commit().await.map_err(|error| error.to_string())?;
        return Ok(None);
    };
    let mut job: EngineJobRun = decode(data)?;
    if job.status != "running" {
        warn!(job_run_id = id, existing_status = %job.status, "finish skipped for terminal job run");
        tx.commit().await.map_err(|error| error.to_string())?;
        return Ok(Some(job));
    }
    job.status = req.status;
    job.input_count = req.input_count;
    job.output_count = req.output_count;
    job.processed_count = req.processed_count;
    job.success_count = req.success_count;
    job.error_count = req.error_count;
    job.metadata = req.metadata;
    job.error_message = req.error_message;
    job.finished_at = Some(now_rfc3339());
    sqlx::query("UPDATE engine_job_runs SET status=$2,finished_at=$3,data=$4 WHERE id=$1")
        .bind(id)
        .bind(&job.status)
        .bind(optional_timestamp(job.finished_at.as_deref())?)
        .bind(json(&job)?)
        .execute(&mut *tx)
        .await
        .map_err(|error| error.to_string())?;
    tx.commit().await.map_err(|error| error.to_string())?;
    info!(job_run_id = id, status = %job.status, "finished Memory Engine job run");
    Ok(Some(job))
}
