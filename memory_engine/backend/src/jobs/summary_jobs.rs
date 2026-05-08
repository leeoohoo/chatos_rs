use tracing::{info, warn};

use crate::config::AppConfig;
use crate::db::Db;
use crate::models::{RunPendingRollupsResponse, RunPendingSummariesResponse};
use crate::repositories::{summaries, threads};
use crate::services::summary;

pub async fn run_pending_thread_summaries(
    config: &AppConfig,
    db: &Db,
) -> Result<RunPendingSummariesResponse, String> {
    run_pending_thread_summaries_with_limit(
        db,
        config,
        None,
        None,
        config.worker_max_threads_per_tick,
    )
    .await
}

pub async fn run_pending_thread_summaries_with_limit(
    db: &Db,
    config: &AppConfig,
    tenant_id: Option<&str>,
    source_id: Option<&str>,
    max_threads: i64,
) -> Result<RunPendingSummariesResponse, String> {
    let pending_threads =
        threads::list_threads_with_pending_records(db, tenant_id, source_id, max_threads).await?;
    if pending_threads.is_empty() {
        return Ok(RunPendingSummariesResponse {
            processed_threads: 0,
            summarized_threads: 0,
        });
    }

    let processed_threads = pending_threads.len();
    let mut generated = 0usize;
    for thread in pending_threads {
        match summary::run_thread_summary(
            config,
            db,
            thread.tenant_id.as_str(),
            thread.source_id.as_str(),
            thread.id.as_str(),
            Some(50),
        )
        .await
        {
            Ok(result) => {
                if result.generated {
                    generated += 1;
                    info!(
                        "[MEMORY-ENGINE-WORKER] summarized thread_id={} source_records={}",
                        result.thread_id, result.source_record_count
                    );
                }
            }
            Err(err) => {
                warn!(
                    "[MEMORY-ENGINE-WORKER] summarize failed thread_id={} error={}",
                    thread.id, err
                );
            }
        }
    }

    Ok(RunPendingSummariesResponse {
        processed_threads,
        summarized_threads: generated,
    })
}

pub async fn run_pending_thread_rollups(
    db: &Db,
    config: &AppConfig,
    tenant_id: Option<&str>,
    source_id: Option<&str>,
    max_threads: i64,
    settings: &summary::RollupSettings,
) -> Result<RunPendingRollupsResponse, String> {
    let pending_threads = summaries::list_threads_with_pending_rollups(
        db,
        tenant_id,
        source_id,
        settings.max_level,
        max_threads,
    )
    .await?;
    if pending_threads.is_empty() {
        return Ok(RunPendingRollupsResponse {
            processed_threads: 0,
            rolled_up_threads: 0,
            generated_summaries: 0,
            marked_summaries: 0,
            failed_threads: 0,
        });
    }

    let mut out = RunPendingRollupsResponse {
        processed_threads: pending_threads.len(),
        rolled_up_threads: 0,
        generated_summaries: 0,
        marked_summaries: 0,
        failed_threads: 0,
    };

    for (tenant_id, source_id, thread_id) in pending_threads {
        match summary::run_thread_rollup(
            config,
            db,
            tenant_id.as_str(),
            source_id.as_str(),
            thread_id.as_str(),
            settings,
        )
        .await
        {
            Ok(result) => {
                if result.generated > 0 {
                    out.rolled_up_threads += 1;
                }
                out.generated_summaries += result.generated;
                out.marked_summaries += result.marked;
                if result.generated > 0 || result.marked > 0 {
                    info!(
                        "[MEMORY-ENGINE-WORKER] rolled up thread_id={} generated={} marked={}",
                        thread_id, result.generated, result.marked
                    );
                }
            }
            Err(err) => {
                out.failed_threads += 1;
                warn!(
                    "[MEMORY-ENGINE-WORKER] rollup failed thread_id={} error={}",
                    thread_id, err
                );
            }
        }
    }

    Ok(out)
}
