use futures_util::{stream, StreamExt};
use tracing::{info, warn};

use crate::config::AppConfig;
use crate::db::Db;
use crate::models::{FinishEngineJobRunRequest, RunPendingSummariesResponse};
use crate::repositories::{control_plane, threads};
use crate::services::summary;

use super::common::{create_scheduler_job_run, finish_job_run, has_recent_scheduler_job_run};

enum SummaryExecutionOutcome {
    Generated {
        thread_id: String,
        source_record_count: usize,
    },
    Noop {
        thread_id: String,
    },
    SlotOccupied {
        thread_id: String,
    },
    Failed {
        thread_id: String,
        error: String,
    },
}

#[allow(dead_code)]
pub async fn run_pending_thread_summaries(
    config: &AppConfig,
    db: &Db,
) -> Result<RunPendingSummariesResponse, String> {
    let policy = control_plane::get_effective_job_policy(db, "summary").await?;
    let limit = policy
        .max_threads_per_tick
        .unwrap_or(config.worker_max_threads_per_tick)
        .max(1);
    run_pending_thread_summaries_with_limit(
        db,
        config,
        None,
        None,
        policy.token_limit.unwrap_or(6000).max(128),
        limit,
    )
    .await
}

pub async fn run_pending_thread_summaries_due(
    db: &Db,
    config: &AppConfig,
    tenant_id: Option<&str>,
    source_id: Option<&str>,
    token_threshold: i64,
    max_threads: i64,
) -> Result<RunPendingSummariesResponse, String> {
    let policy = control_plane::get_effective_job_policy(db, "summary").await?;
    let interval_seconds = policy.interval_seconds.unwrap_or(30).max(3);
    if has_recent_scheduler_job_run(db, "summary", tenant_id, source_id, interval_seconds).await? {
        return Ok(empty_response());
    }
    run_pending_thread_summaries_internal(
        db,
        config,
        tenant_id,
        source_id,
        token_threshold,
        max_threads,
    )
    .await
}

pub async fn run_pending_thread_summaries_with_limit(
    db: &Db,
    config: &AppConfig,
    tenant_id: Option<&str>,
    source_id: Option<&str>,
    token_threshold: i64,
    max_threads: i64,
) -> Result<RunPendingSummariesResponse, String> {
    run_pending_thread_summaries_internal(
        db,
        config,
        tenant_id,
        source_id,
        token_threshold,
        max_threads,
    )
    .await
}

async fn run_pending_thread_summaries_internal(
    db: &Db,
    config: &AppConfig,
    tenant_id: Option<&str>,
    source_id: Option<&str>,
    token_threshold: i64,
    max_threads: i64,
) -> Result<RunPendingSummariesResponse, String> {
    let pending_threads = threads::list_threads_with_pending_records_by_token_threshold(
        db,
        tenant_id,
        source_id,
        token_threshold,
        max_threads,
    )
    .await?;
    if pending_threads.is_empty() {
        return Ok(empty_response());
    }

    let processed_threads = pending_threads.len();
    let concurrency = summary_execution_concurrency(config, max_threads);
    let job_run = create_scheduler_job_run(db, "summary", tenant_id, source_id, None).await?;
    let result: Result<RunPendingSummariesResponse, String> = async {
        let db = db.clone();
        let config = config.clone();
        let execution_results = stream::iter(pending_threads.into_iter().map(|thread| {
            let db = db.clone();
            let config = config.clone();
            async move {
                let thread_id = thread.id.clone();
                match summary::run_thread_summary_with_thread(
                    &config,
                    &db,
                    thread,
                    summary::SCHEDULER_TRIGGER,
                )
                .await {
                    Ok(result) if result.generated => SummaryExecutionOutcome::Generated {
                        thread_id: result.thread_id,
                        source_record_count: result.source_record_count,
                    },
                    Ok(result) => SummaryExecutionOutcome::Noop {
                        thread_id: result.thread_id,
                    },
                    Err(err) if err.contains("slot already occupied") => {
                        SummaryExecutionOutcome::SlotOccupied { thread_id }
                    }
                    Err(err) => SummaryExecutionOutcome::Failed {
                        thread_id,
                        error: err,
                    },
                }
            }
        }))
        .buffer_unordered(concurrency)
        .collect::<Vec<_>>()
        .await;

        let mut generated = 0usize;
        for outcome in execution_results {
            match outcome {
                SummaryExecutionOutcome::Generated {
                    thread_id,
                    source_record_count,
                } => {
                    generated += 1;
                    info!(
                        "[MEMORY-ENGINE-WORKER] summarized thread_id={} source_records={}",
                        thread_id, source_record_count
                    );
                }
                SummaryExecutionOutcome::Noop { thread_id } => {
                    info!(
                        "[MEMORY-ENGINE-WORKER] summary noop thread_id={} because no eligible records remained",
                        thread_id
                    );
                }
                SummaryExecutionOutcome::SlotOccupied { thread_id } => {
                    info!(
                        "[MEMORY-ENGINE-WORKER] skip thread_id={} because summary slot is occupied",
                        thread_id
                    );
                }
                SummaryExecutionOutcome::Failed { thread_id, error } => {
                    warn!(
                        "[MEMORY-ENGINE-WORKER] summarize failed thread_id={} error={}",
                        thread_id, error
                    );
                }
            }
        }

        Ok(RunPendingSummariesResponse {
            processed_threads,
            summarized_threads: generated,
        })
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
                    input_count: processed_threads as i64,
                    output_count: 0,
                    processed_count: processed_threads as i64,
                    success_count: 0,
                    error_count: 1,
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
            status: "done".to_string(),
            input_count: response.processed_threads as i64,
            output_count: response.summarized_threads as i64,
            processed_count: response.processed_threads as i64,
            success_count: response.summarized_threads as i64,
            error_count: (response
                .processed_threads
                .saturating_sub(response.summarized_threads)) as i64,
            metadata: None,
            error_message: None,
        },
    )
    .await;

    Ok(response)
}

fn empty_response() -> RunPendingSummariesResponse {
    RunPendingSummariesResponse {
        processed_threads: 0,
        summarized_threads: 0,
    }
}

fn summary_execution_concurrency(config: &AppConfig, max_threads: i64) -> usize {
    max_threads
        .max(1)
        .min(config.worker_summary_concurrency.max(1) as i64) as usize
}
