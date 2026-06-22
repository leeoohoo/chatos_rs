use futures_util::TryStreamExt;
use mongodb::bson::doc;
use tracing::warn;

use crate::db::Db;
use crate::models::{EngineJobRun, now_rfc3339};
use crate::repositories::threads;

use super::super::common::{
    JOB_TYPE_THREAD_REPAIR, STALE_THREAD_REPAIR_JOB_TIMEOUT_SECS, job_run_collection,
};

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
    let running_jobs = job_run_collection(db)
        .find(doc! {"status": "running"})
        .await
        .map_err(|err| err.to_string())?
        .try_collect::<Vec<EngineJobRun>>()
        .await
        .map_err(|err| err.to_string())?;

    if running_jobs.is_empty() {
        return Ok(0);
    }

    let stale_jobs = running_jobs
        .into_iter()
        .filter(|job| is_stale_running_job(job, timeout_secs))
        .collect::<Vec<_>>();
    if stale_jobs.is_empty() {
        return Ok(0);
    }

    let stale_job_ids = stale_jobs
        .iter()
        .map(|job| job.id.clone())
        .collect::<Vec<_>>();

    let result = job_run_collection(db)
        .update_many(
            doc! {
                "status": "running",
                "id": {"$in": stale_job_ids},
            },
            doc! {
                "$set": {
                    "status": "failed",
                    "finished_at": finished_at,
                    "error_message": "job run was marked failed automatically because it stayed in running status past the timeout",
                }
            },
        )
        .await
        .map_err(|err| err.to_string())?;

    if result.modified_count > 0 {
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
                        "pending",
                        None,
                        None,
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

    Ok(result.modified_count as i64)
}
