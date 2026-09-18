// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use sqlx::types::Json;
use tracing::warn;

use crate::db::Db;
use crate::models::{now_rfc3339, EngineJobRun};
use crate::repositories::postgres::{decode, json, optional_timestamp};
use crate::repositories::threads;

use super::super::common::{JOB_TYPE_THREAD_REPAIR, STALE_THREAD_REPAIR_JOB_TIMEOUT_SECS};

fn stale_timeout_secs(job: &EngineJobRun, default_timeout_secs: i64) -> i64 {
    if job.job_type == JOB_TYPE_THREAD_REPAIR {
        STALE_THREAD_REPAIR_JOB_TIMEOUT_SECS
    } else {
        default_timeout_secs.max(30)
    }
}

fn is_stale_running_job(job: &EngineJobRun, default_timeout_secs: i64) -> bool {
    let timeout = stale_timeout_secs(job, default_timeout_secs);
    let stale_before = (chrono::Utc::now() - chrono::Duration::seconds(timeout)).to_rfc3339();
    job.started_at < stale_before
}

pub async fn fail_stale_running_job_runs(db: &Db, timeout_secs: i64) -> Result<i64, String> {
    let finished_at = now_rfc3339();
    let running_jobs = sqlx::query_scalar::<_, Json<serde_json::Value>>(
        "SELECT data FROM engine_job_runs WHERE status='running'",
    )
    .fetch_all(db)
    .await
    .map_err(|error| error.to_string())?
    .into_iter()
    .map(decode)
    .collect::<Result<Vec<EngineJobRun>, _>>()?;

    if running_jobs.is_empty() {
        return Ok(0);
    }

    let mut stale_jobs = running_jobs
        .into_iter()
        .filter(|job| is_stale_running_job(job, timeout_secs))
        .collect::<Vec<_>>();
    if stale_jobs.is_empty() {
        return Ok(0);
    }

    let mut tx = db.begin().await.map_err(|error| error.to_string())?;
    let mut modified = 0_i64;
    for job in &mut stale_jobs {
        job.status = "failed".to_string();
        job.finished_at = Some(finished_at.clone());
        job.error_message = Some(
            "job run was marked failed automatically because it stayed in running status past the timeout"
                .to_string(),
        );
        let result = sqlx::query(
            "UPDATE engine_job_runs SET status='failed',finished_at=$2,data=$3 \
             WHERE id=$1 AND status='running'",
        )
        .bind(&job.id)
        .bind(optional_timestamp(job.finished_at.as_deref())?)
        .bind(json(job)?)
        .execute(&mut *tx)
        .await
        .map_err(|error| error.to_string())?;
        modified =
            modified.saturating_add(i64::try_from(result.rows_affected()).unwrap_or(i64::MAX));
    }
    tx.commit().await.map_err(|error| error.to_string())?;

    if modified > 0 {
        for job in &stale_jobs {
            if job.job_type == "summary" {
                if let (Some(tenant_id), Some(source_id), Some(thread_id)) = (
                    job.tenant_id.as_deref(),
                    job.source_id.as_deref(),
                    job.thread_id.as_deref(),
                ) {
                    let _ = threads::release_summary_slot(
                        db,
                        tenant_id,
                        source_id,
                        thread_id,
                        job.id.as_str(),
                        0,
                        0,
                    )
                    .await;
                }
            } else if job.job_type == "rollup" {
                if let (Some(tenant_id), Some(source_id), Some(thread_id)) = (
                    job.tenant_id.as_deref(),
                    job.source_id.as_deref(),
                    job.thread_id.as_deref(),
                ) {
                    let _ = threads::release_rollup_slot(
                        db,
                        tenant_id,
                        source_id,
                        thread_id,
                        job.id.as_str(),
                    )
                    .await;
                }
            }
            warn!(
                "[MEMORY-ENGINE-JOB] stale-auto-failed job_run_id={} job_type={} trigger_type={} tenant_id={} source_id={} thread_id={} subject_id={} started_at={} timeout_secs={}",
                job.id,
                job.job_type,
                job.trigger_type,
                job.tenant_id.as_deref().unwrap_or("-"),
                job.source_id.as_deref().unwrap_or("-"),
                job.thread_id.as_deref().unwrap_or("-"),
                job.subject_id.as_deref().unwrap_or("-"),
                job.started_at,
                stale_timeout_secs(job, timeout_secs)
            );
        }
    }

    Ok(modified)
}
