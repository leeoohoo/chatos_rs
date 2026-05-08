use tracing::{info, warn};

use crate::config::AppConfig;
use crate::db::Db;
use crate::models::{CreateEngineJobRunRequest, FinishEngineJobRunRequest};
use crate::models::{RunPendingRollupsResponse, RunPendingSummariesResponse};
use crate::repositories::{summaries, threads};
use crate::repositories::control_plane;
use crate::services::{control_plane as cp_service, summary};

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
        limit,
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
    let job_run = control_plane::create_job_run(
        db,
        CreateEngineJobRunRequest {
            job_type: "summary".to_string(),
            trigger_type: "scheduler".to_string(),
            tenant_id: tenant_id.map(|value| value.to_string()),
            source_id: source_id.map(|value| value.to_string()),
            thread_id: None,
            subject_id: None,
            thread_label: None,
            metadata: None,
        },
    )
    .await?;
    let pending_threads =
        threads::list_threads_with_pending_records(db, tenant_id, source_id, max_threads).await?;
    if pending_threads.is_empty() {
        let _ = control_plane::finish_job_run(
            db,
            job_run.id.as_str(),
            FinishEngineJobRunRequest {
                status: "done".to_string(),
                input_count: 0,
                output_count: 0,
                processed_count: 0,
                success_count: 0,
                error_count: 0,
                metadata: None,
                error_message: None,
            },
        )
        .await;
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

    let response = RunPendingSummariesResponse {
        processed_threads,
        summarized_threads: generated,
    };

    let _ = control_plane::finish_job_run(
        db,
        job_run.id.as_str(),
        FinishEngineJobRunRequest {
            status: "done".to_string(),
            input_count: processed_threads as i64,
            output_count: generated as i64,
            processed_count: processed_threads as i64,
            success_count: generated as i64,
            error_count: (processed_threads.saturating_sub(generated)) as i64,
            metadata: None,
            error_message: None,
        },
    )
    .await;

    Ok(response)
}

pub async fn run_pending_thread_rollups(
    db: &Db,
    config: &AppConfig,
    tenant_id: Option<&str>,
    source_id: Option<&str>,
    max_threads: i64,
    settings: &summary::RollupSettings,
) -> Result<RunPendingRollupsResponse, String> {
    let job_run = control_plane::create_job_run(
        db,
        CreateEngineJobRunRequest {
            job_type: "rollup".to_string(),
            trigger_type: "scheduler".to_string(),
            tenant_id: tenant_id.map(|value| value.to_string()),
            source_id: source_id.map(|value| value.to_string()),
            thread_id: None,
            subject_id: None,
            thread_label: None,
            metadata: cp_service::merge_metadata(None, cp_service::policy_meta(&control_plane::get_effective_job_policy(db, "rollup").await?)),
        },
    )
    .await?;
    let pending_threads = summaries::list_threads_with_pending_rollups(
        db,
        tenant_id,
        source_id,
        settings.max_level,
        max_threads,
    )
    .await?;
    if pending_threads.is_empty() {
        let _ = control_plane::finish_job_run(
            db,
            job_run.id.as_str(),
            FinishEngineJobRunRequest {
                status: "done".to_string(),
                input_count: 0,
                output_count: 0,
                processed_count: 0,
                success_count: 0,
                error_count: 0,
                metadata: None,
                error_message: None,
            },
        )
        .await;
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

    let _ = control_plane::finish_job_run(
        db,
        job_run.id.as_str(),
        FinishEngineJobRunRequest {
            status: if out.failed_threads > 0 && out.rolled_up_threads == 0 {
                "failed".to_string()
            } else {
                "done".to_string()
            },
            input_count: out.processed_threads as i64,
            output_count: out.generated_summaries as i64,
            processed_count: out.processed_threads as i64,
            success_count: out.rolled_up_threads as i64,
            error_count: out.failed_threads as i64,
            metadata: None,
            error_message: None,
        },
    )
    .await;

    Ok(out)
}
