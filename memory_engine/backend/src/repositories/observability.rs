// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use tokio::try_join;

use crate::db::Db;
use crate::models::{
    MemoryEngineBacklogStats, MemoryEngineReconcileBacklogStats, MemoryEngineRollupBacklogStats,
    MemoryEngineSummaryBacklogStats,
};

const DEFAULT_STALE_JOB_TIMEOUT_SECS: i64 = 300;
const THREAD_REPAIR_STALE_JOB_TIMEOUT_SECS: i64 = 1800;

pub async fn system_backlog_stats(db: &Db) -> Result<MemoryEngineBacklogStats, String> {
    let (summary, rollup, reconcile) = try_join!(
        summary_backlog_stats(db),
        rollup_backlog_stats(db),
        reconcile_backlog_stats(db),
    )?;
    Ok(MemoryEngineBacklogStats {
        summary,
        rollup,
        reconcile,
    })
}

async fn summary_backlog_stats(db: &Db) -> Result<MemoryEngineSummaryBacklogStats, String> {
    let (pending_threads, pending_records, pending_tokens) = sqlx::query_as::<_, (i64, i64, i64)>(
        "SELECT COUNT(*),COALESCE(SUM(pending_record_count),0)::BIGINT, \
             COALESCE(SUM(pending_summary_tokens),0)::BIGINT \
             FROM engine_threads WHERE pending_record_count>0",
    )
    .fetch_one(db)
    .await
    .map_err(|error| error.to_string())?;
    Ok(MemoryEngineSummaryBacklogStats {
        pending_threads,
        pending_records,
        pending_tokens,
    })
}

async fn rollup_backlog_stats(db: &Db) -> Result<MemoryEngineRollupBacklogStats, String> {
    let (pending_summaries, pending_threads) = sqlx::query_as::<_, (i64, i64)>(
        "SELECT COUNT(*),COUNT(DISTINCT (tenant_id,source_id,thread_id)) \
         FROM engine_summaries WHERE summary_type='thread_incremental' \
         AND status='done' AND rollup_status='pending'",
    )
    .fetch_one(db)
    .await
    .map_err(|error| error.to_string())?;
    Ok(MemoryEngineRollupBacklogStats {
        pending_summaries,
        pending_threads,
    })
}

async fn reconcile_backlog_stats(db: &Db) -> Result<MemoryEngineReconcileBacklogStats, String> {
    let now = chrono::Utc::now();
    let default_stale_before = now - chrono::Duration::seconds(DEFAULT_STALE_JOB_TIMEOUT_SECS);
    let repair_stale_before = now - chrono::Duration::seconds(THREAD_REPAIR_STALE_JOB_TIMEOUT_SECS);
    let (candidate_threads, running_jobs, stale_running_jobs) = try_join!(
        sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM engine_threads \
             WHERE summary_status='pending' AND pending_record_count>0"
        )
        .fetch_one(db),
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM engine_job_runs WHERE status='running'")
            .fetch_one(db),
        sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM engine_job_runs WHERE status='running' AND \
             ((job_type='thread_repair' AND started_at<$1) OR \
              (job_type<>'thread_repair' AND started_at<$2))"
        )
        .bind(repair_stale_before)
        .bind(default_stale_before)
        .fetch_one(db),
    )
    .map_err(|error| error.to_string())?;
    Ok(MemoryEngineReconcileBacklogStats {
        candidate_threads,
        running_jobs,
        stale_running_jobs,
    })
}
