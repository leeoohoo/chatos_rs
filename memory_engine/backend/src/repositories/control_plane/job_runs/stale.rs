// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashSet;

use sqlx::types::Json;
use tracing::warn;

use crate::db::Db;
use crate::models::{now_rfc3339, EngineJobRun};
use crate::repositories::postgres::{decode, json, optional_timestamp};
use crate::repositories::thread_partitions;

use super::super::common::{LEGACY_JOB_TYPE_THREAD_REPAIR, STALE_LEGACY_REPAIR_JOB_TIMEOUT_SECS};

fn stale_timeout_secs(job: &EngineJobRun, default_timeout_secs: i64) -> i64 {
    if job.job_type == LEGACY_JOB_TYPE_THREAD_REPAIR {
        STALE_LEGACY_REPAIR_JOB_TIMEOUT_SECS
    } else {
        default_timeout_secs.max(30)
    }
}

fn is_stale_running_job(
    job: &EngineJobRun,
    default_timeout_secs: i64,
    cloud_agent_managed: bool,
) -> bool {
    if cloud_agent_managed {
        return false;
    }
    let timeout = stale_timeout_secs(job, default_timeout_secs);
    let stale_before = (chrono::Utc::now() - chrono::Duration::seconds(timeout)).to_rfc3339();
    job.started_at < stale_before
}

async fn cloud_agent_managed_job_ids(db: &Db) -> Result<HashSet<String>, String> {
    sqlx::query_scalar::<_, String>(
        "SELECT DISTINCT state.data->>'owner_entity_id' \
           FROM engine_cloud_agent_run_states state \
           JOIN engine_job_runs job ON job.id=state.data->>'owner_entity_id' \
                AND job.status='running' \
           LEFT JOIN cloud_agent_outbox outbox ON outbox.agent_run_id=state.agent_run_id \
                AND outbox.data->>'topic'='owner_lifecycle_terminal' \
                AND outbox.status IN ('pending','publishing') \
          WHERE state.data->>'owner_service'='memory-engine' \
            AND state.data->>'owner_entity_type' IN ( \
                'summary_job_run','rollup_job_run','subject_memory_job_run') \
            AND NULLIF(state.data->>'owner_entity_id','') IS NOT NULL \
            AND (state.data->>'status' NOT IN ('succeeded','failed','blocked','cancelled') \
                 OR outbox.event_id IS NOT NULL) ",
    )
    .fetch_all(db)
    .await
    .map(|ids| ids.into_iter().collect())
    .map_err(|error| error.to_string())
}

pub async fn fail_stale_running_job_runs(db: &Db, timeout_secs: i64) -> Result<i64, String> {
    let finished_at = now_rfc3339();
    let candidate_timeout_secs = timeout_secs.clamp(30, STALE_LEGACY_REPAIR_JOB_TIMEOUT_SECS);
    let running_jobs = sqlx::query_scalar::<_, Json<serde_json::Value>>(
        "SELECT data FROM engine_job_runs \
         WHERE status='running' AND started_at < now()-($1 * interval '1 second')",
    )
    .bind(candidate_timeout_secs)
    .fetch_all(db)
    .await
    .map_err(|error| error.to_string())?
    .into_iter()
    .map(decode)
    .collect::<Result<Vec<EngineJobRun>, _>>()?;

    if running_jobs.is_empty() {
        return Ok(0);
    }

    let cloud_agent_managed_jobs = cloud_agent_managed_job_ids(db).await?;

    let mut stale_jobs = running_jobs
        .into_iter()
        .filter(|job| {
            is_stale_running_job(
                job,
                timeout_secs,
                cloud_agent_managed_jobs.contains(&job.id),
            )
        })
        .collect::<Vec<_>>();
    if stale_jobs.is_empty() {
        return Ok(0);
    }

    let mut tx = db.begin().await.map_err(|error| error.to_string())?;
    let mut modified = 0_i64;
    let mut failed_jobs = Vec::new();
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
        if result.rows_affected() == 1 {
            failed_jobs.push(job.clone());
        }
    }
    tx.commit().await.map_err(|error| error.to_string())?;

    if modified > 0 {
        for job in &failed_jobs {
            if job.job_type == "summary" {
                if let (Some(tenant_id), Some(source_id), Some(thread_id)) = (
                    job.tenant_id.as_deref(),
                    job.source_id.as_deref(),
                    job.thread_id.as_deref(),
                ) {
                    let _ = thread_partitions::release_summary_claim_by_job(
                        db,
                        tenant_id,
                        source_id,
                        thread_id,
                        job.id.as_str(),
                    )
                    .await;
                }
            } else if job.job_type == "rollup" {
                if let (Some(tenant_id), Some(source_id), Some(thread_id)) = (
                    job.tenant_id.as_deref(),
                    job.source_id.as_deref(),
                    job.thread_id.as_deref(),
                ) {
                    let _ = thread_partitions::release_rollup_claim_by_job(
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

#[cfg(test)]
mod tests {
    use super::is_stale_running_job;
    use crate::models::EngineJobRun;

    fn old_job() -> EngineJobRun {
        EngineJobRun {
            id: "job-1".to_string(),
            job_type: "summary".to_string(),
            trigger_type: "scheduler".to_string(),
            tenant_id: Some("tenant".to_string()),
            source_id: Some("source".to_string()),
            thread_id: Some("thread".to_string()),
            subject_id: Some("agent:test".to_string()),
            thread_label: None,
            thread_display_name: None,
            status: "running".to_string(),
            input_count: 0,
            output_count: 0,
            processed_count: 0,
            success_count: 0,
            error_count: 0,
            metadata: None,
            error_message: None,
            started_at: "2020-01-01T00:00:00Z".to_string(),
            finished_at: None,
        }
    }

    #[test]
    fn durable_cloud_agent_ownership_prevents_parent_job_timeout() {
        let job = old_job();
        assert!(is_stale_running_job(&job, 300, false));
        assert!(!is_stale_running_job(&job, 300, true));
    }
}
