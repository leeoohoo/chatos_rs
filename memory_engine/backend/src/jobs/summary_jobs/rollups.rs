use futures_util::{stream, StreamExt};
use tracing::{info, warn};

use crate::config::AppConfig;
use crate::db::Db;
use crate::models::{FinishEngineJobRunRequest, RunPendingRollupsResponse};
use crate::repositories::{control_plane, summaries};
use crate::services::{control_plane as cp_service, summary};

use super::common::{create_scheduler_job_run, finish_job_run, has_recent_scheduler_job_run};

enum RollupExecutionOutcome {
    Success {
        thread_id: String,
        batches: usize,
        generated: usize,
        marked: usize,
    },
    Failed {
        thread_id: String,
        batches: usize,
        generated: usize,
        marked: usize,
        error: String,
    },
}

pub async fn run_pending_thread_rollups_due(
    db: &Db,
    config: &AppConfig,
    tenant_id: Option<&str>,
    source_id: Option<&str>,
    max_threads: i64,
    settings: &summary::RollupSettings,
) -> Result<RunPendingRollupsResponse, String> {
    let policy = control_plane::get_effective_job_policy(db, "rollup").await?;
    let interval_seconds = policy.interval_seconds.unwrap_or(60).max(3);
    if has_recent_scheduler_job_run(db, "rollup", tenant_id, source_id, interval_seconds).await? {
        return Ok(empty_response());
    }
    run_pending_thread_rollups_internal(db, config, tenant_id, source_id, max_threads, settings)
        .await
}

pub async fn run_pending_thread_rollups(
    db: &Db,
    config: &AppConfig,
    tenant_id: Option<&str>,
    source_id: Option<&str>,
    max_threads: i64,
    settings: &summary::RollupSettings,
) -> Result<RunPendingRollupsResponse, String> {
    run_pending_thread_rollups_internal(db, config, tenant_id, source_id, max_threads, settings)
        .await
}

async fn run_pending_thread_rollups_internal(
    db: &Db,
    config: &AppConfig,
    tenant_id: Option<&str>,
    source_id: Option<&str>,
    max_threads: i64,
    settings: &summary::RollupSettings,
) -> Result<RunPendingRollupsResponse, String> {
    let mut out = empty_response();
    let candidate_threads = summaries::list_threads_with_pending_rollups(
        db,
        tenant_id,
        source_id,
        settings.max_level,
        max_threads,
    )
    .await?;
    let mut pending_threads = Vec::new();
    for (tenant_id, source_id, thread_id) in candidate_threads {
        if summary::prepare_thread_rollup(
            db,
            tenant_id.as_str(),
            source_id.as_str(),
            thread_id.as_str(),
            settings,
        )
        .await?
        .is_some()
        {
            pending_threads.push((tenant_id, source_id, thread_id));
        }
    }
    if pending_threads.is_empty() {
        return Ok(out);
    }

    let concurrency = rollup_execution_concurrency(config, max_threads);
    let policy = control_plane::get_effective_job_policy(db, "rollup").await?;
    let job_run = create_scheduler_job_run(
        db,
        "rollup",
        tenant_id,
        source_id,
        cp_service::merge_metadata(None, cp_service::policy_meta(&policy)),
    )
    .await?;
    let result: Result<RunPendingRollupsResponse, String> = async {
        out.processed_threads = pending_threads.len();
        let db = db.clone();
        let config = config.clone();
        let settings = settings.clone();
        let execution_results = stream::iter(pending_threads.into_iter().map(
            |(tenant_id, source_id, thread_id)| {
                let db = db.clone();
                let config = config.clone();
                let settings = settings.clone();
                async move {
                    match summary::run_thread_rollups_until_drained(
                        &config,
                        &db,
                        tenant_id.as_str(),
                        source_id.as_str(),
                        thread_id.as_str(),
                        &settings,
                        summary::SCHEDULER_TRIGGER,
                    )
                    .await
                    {
                        Ok(result) => RollupExecutionOutcome::Success {
                            thread_id,
                            batches: result.batches,
                            generated: result.generated,
                            marked: result.marked,
                        },
                        Err(err) => RollupExecutionOutcome::Failed {
                            thread_id,
                            batches: err.batches,
                            generated: err.generated,
                            marked: err.marked,
                            error: err.error,
                        },
                    }
                }
            },
        ))
        .buffer_unordered(concurrency)
        .collect::<Vec<_>>()
        .await;

        for outcome in execution_results {
            match outcome {
                RollupExecutionOutcome::Success {
                    thread_id,
                    batches,
                    generated,
                    marked,
                } => {
                    if batches > 0 {
                        out.rolled_up_threads += 1;
                    }
                    out.generated_summaries += generated;
                    out.marked_summaries += marked;
                    if generated > 0 || marked > 0 {
                        info!(
                            "[MEMORY-ENGINE-WORKER] rolled up thread_id={} batches={} generated={} marked={}",
                            thread_id, batches, generated, marked
                        );
                    }
                }
                RollupExecutionOutcome::Failed {
                    thread_id,
                    batches,
                    generated,
                    marked,
                    error,
                } => {
                    out.failed_threads += 1;
                    if batches > 0 {
                        out.rolled_up_threads += 1;
                    }
                    out.generated_summaries += generated;
                    out.marked_summaries += marked;
                    warn!(
                        "[MEMORY-ENGINE-WORKER] rollup failed thread_id={} batches={} generated={} marked={} error={}",
                        thread_id, batches, generated, marked, error
                    );
                }
            }
        }

        Ok(out.clone())
    }
    .await;

    let response = match &result {
        Ok(response) => response.clone(),
        Err(err) => {
            finish_job_run(
                db,
                job_run.id.as_str(),
                FinishEngineJobRunRequest {
                    status: "failed".to_string(),
                    input_count: out.processed_threads as i64,
                    output_count: out.generated_summaries as i64,
                    processed_count: out.processed_threads as i64,
                    success_count: out.rolled_up_threads as i64,
                    error_count: out.failed_threads.max(1) as i64,
                    metadata: None,
                    error_message: Some(err.clone()),
                },
            )
            .await;
            return result;
        }
    };

    finish_job_run(
        db,
        job_run.id.as_str(),
        FinishEngineJobRunRequest {
            status: if response.failed_threads > 0 && response.rolled_up_threads == 0 {
                "failed".to_string()
            } else {
                "done".to_string()
            },
            input_count: response.processed_threads as i64,
            output_count: response.generated_summaries as i64,
            processed_count: response.processed_threads as i64,
            success_count: response.rolled_up_threads as i64,
            error_count: response.failed_threads as i64,
            metadata: None,
            error_message: None,
        },
    )
    .await;

    Ok(response)
}

fn empty_response() -> RunPendingRollupsResponse {
    RunPendingRollupsResponse {
        processed_threads: 0,
        rolled_up_threads: 0,
        generated_summaries: 0,
        marked_summaries: 0,
        failed_threads: 0,
    }
}

fn rollup_execution_concurrency(config: &AppConfig, max_threads: i64) -> usize {
    max_threads
        .max(1)
        .min(config.worker_rollup_concurrency.max(1) as i64) as usize
}
