// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use futures_util::TryStreamExt;
use mongodb::bson::{doc, Document};
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
    let pipeline = vec![
        doc! {
            "$match": {
                "pending_record_count": {"$gt": 0},
            }
        },
        doc! {
            "$group": {
                "_id": mongodb::bson::Bson::Null,
                "pending_threads": {"$sum": 1},
                "pending_records": {"$sum": {"$ifNull": ["$pending_record_count", 0]}},
                "pending_tokens": {"$sum": {"$ifNull": ["$pending_summary_tokens", 0]}},
            }
        },
    ];
    let row = aggregate_one(db, "engine_threads", pipeline).await?;

    Ok(MemoryEngineSummaryBacklogStats {
        pending_threads: row_i64(row.as_ref(), "pending_threads"),
        pending_records: row_i64(row.as_ref(), "pending_records"),
        pending_tokens: row_i64(row.as_ref(), "pending_tokens"),
    })
}

async fn rollup_backlog_stats(db: &Db) -> Result<MemoryEngineRollupBacklogStats, String> {
    let filter = doc! {
        "summary_type": "thread_incremental",
        "status": "done",
        "rollup_status": "pending",
    };
    let pending_summaries = db
        .collection::<Document>("engine_summaries")
        .count_documents(filter.clone())
        .await
        .map_err(|err| err.to_string())? as i64;
    let pipeline = vec![
        doc! {"$match": filter},
        doc! {
            "$group": {
                "_id": {
                    "tenant_id": "$tenant_id",
                    "source_id": "$source_id",
                    "thread_id": "$thread_id",
                }
            }
        },
        doc! {"$count": "pending_threads"},
    ];
    let row = aggregate_one(db, "engine_summaries", pipeline).await?;

    Ok(MemoryEngineRollupBacklogStats {
        pending_summaries,
        pending_threads: row_i64(row.as_ref(), "pending_threads"),
    })
}

async fn reconcile_backlog_stats(db: &Db) -> Result<MemoryEngineReconcileBacklogStats, String> {
    let now = chrono::Utc::now();
    let default_stale_before =
        (now - chrono::Duration::seconds(DEFAULT_STALE_JOB_TIMEOUT_SECS)).to_rfc3339();
    let repair_stale_before =
        (now - chrono::Duration::seconds(THREAD_REPAIR_STALE_JOB_TIMEOUT_SECS)).to_rfc3339();
    let threads = db.collection::<Document>("engine_threads");
    let jobs = db.collection::<Document>("engine_job_runs");
    let (candidate_threads, running_jobs, stale_running_jobs) = try_join!(
        threads.count_documents(doc! {
            "summary_status": "pending",
            "pending_record_count": {"$gt": 0},
        }),
        jobs.count_documents(doc! {"status": "running"}),
        jobs.count_documents(doc! {
            "status": "running",
            "$or": [
                {
                    "job_type": "thread_repair",
                    "started_at": {"$lt": repair_stale_before},
                },
                {
                    "job_type": {"$ne": "thread_repair"},
                    "started_at": {"$lt": default_stale_before},
                },
            ],
        }),
    )
    .map_err(|err| err.to_string())?;

    Ok(MemoryEngineReconcileBacklogStats {
        candidate_threads: candidate_threads as i64,
        running_jobs: running_jobs as i64,
        stale_running_jobs: stale_running_jobs as i64,
    })
}

async fn aggregate_one(
    db: &Db,
    collection: &str,
    pipeline: Vec<Document>,
) -> Result<Option<Document>, String> {
    db.collection::<Document>(collection)
        .aggregate(pipeline)
        .await
        .map_err(|err| err.to_string())?
        .try_next()
        .await
        .map_err(|err| err.to_string())
}

fn row_i64(row: Option<&Document>, key: &str) -> i64 {
    row.and_then(|row| row.get_i64(key).ok())
        .or_else(|| row.and_then(|row| row.get_i32(key).ok().map(i64::from)))
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use mongodb::bson::doc;

    use super::row_i64;

    #[test]
    fn row_i64_supports_mongo_integer_types_and_missing_rows() {
        assert_eq!(row_i64(Some(&doc! {"count": 3i64}), "count"), 3);
        assert_eq!(row_i64(Some(&doc! {"count": 4i32}), "count"), 4);
        assert_eq!(row_i64(Some(&doc! {}), "count"), 0);
        assert_eq!(row_i64(None, "count"), 0);
    }
}
