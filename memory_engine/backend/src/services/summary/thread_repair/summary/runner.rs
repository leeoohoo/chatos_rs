// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use tokio::task;
use tracing::{info, warn};

use crate::config::AppConfig;
use crate::db::Db;
use crate::models::{
    CreateEngineJobRunRequest, FinishEngineJobRunRequest, RunThreadRepairSummaryResponse,
};
use crate::repositories::{control_plane as cp_repo, records, summaries, threads};

use super::super::super::builders::build_repair_summary_text;
use super::super::super::render::decorate_generated_text;
use super::super::common::{load_repair_summary_preparation, THREAD_REPAIR_JOB_TYPE};
use super::metadata::{
    failed_metadata, finalize_job_run, noop_metadata, noop_response, running_response,
    start_metadata, success_metadata, success_response,
};

pub async fn run_thread_repair_summary(
    config: &AppConfig,
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    thread_id: &str,
) -> Result<RunThreadRepairSummaryResponse, String> {
    let running_jobs = cp_repo::list_job_runs(
        db,
        Some(THREAD_REPAIR_JOB_TYPE),
        None,
        Some(thread_id),
        Some("running"),
        Some(tenant_id),
        Some(source_id),
        10,
    )
    .await?;
    if let Some(existing_job) = running_jobs.into_iter().next() {
        info!(
            "[MEMORY-ENGINE-REPAIR] reuse-running tenant_id={} source_id={} thread_id={} job_run_id={}",
            tenant_id, source_id, thread_id, existing_job.id
        );
        return Ok(running_response(thread_id, existing_job.id, 0));
    }

    let prep = load_repair_summary_preparation(db, tenant_id, source_id, thread_id).await?;
    if prep.selection.selected.is_empty() {
        return Ok(noop_response(thread_id, None));
    }

    let job_run = cp_repo::create_job_run(
        db,
        CreateEngineJobRunRequest {
            job_type: THREAD_REPAIR_JOB_TYPE.to_string(),
            trigger_type: "thread_direct".to_string(),
            tenant_id: Some(tenant_id.to_string()),
            source_id: Some(source_id.to_string()),
            thread_id: Some(thread_id.to_string()),
            subject_id: Some(prep.thread.subject_id.clone()),
            thread_label: None,
            metadata: Some(start_metadata(prep.pending_before_count)),
        },
    )
    .await?;
    info!(
        "[MEMORY-ENGINE-REPAIR] job-created tenant_id={} source_id={} thread_id={} job_run_id={}",
        tenant_id, source_id, thread_id, job_run.id
    );

    spawn_detached_repair_job(
        config,
        db,
        tenant_id,
        source_id,
        thread_id,
        job_run.id.as_str(),
    );

    Ok(running_response(
        thread_id,
        job_run.id,
        prep.selection.selected.len(),
    ))
}

fn spawn_detached_repair_job(
    config: &AppConfig,
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    thread_id: &str,
    job_run_id: &str,
) {
    let config_cloned = config.clone();
    let db_cloned = db.clone();
    let tenant_id_owned = tenant_id.to_string();
    let source_id_owned = source_id.to_string();
    let thread_id_owned = thread_id.to_string();
    let job_run_id_owned = job_run_id.to_string();
    task::spawn(async move {
        if let Err(err) = run_thread_repair_summary_job(
            &config_cloned,
            &db_cloned,
            tenant_id_owned.as_str(),
            source_id_owned.as_str(),
            thread_id_owned.as_str(),
            job_run_id_owned.as_str(),
        )
        .await
        {
            warn!(
                "[MEMORY-ENGINE-REPAIR] detached-run-failed thread_id={} job_run_id={} error={}",
                thread_id_owned, job_run_id_owned, err
            );
        }
    });
}

async fn run_thread_repair_summary_job(
    config: &AppConfig,
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    thread_id: &str,
    job_run_id: &str,
) -> Result<RunThreadRepairSummaryResponse, String> {
    let prep = load_repair_summary_preparation(db, tenant_id, source_id, thread_id).await?;
    if prep.selection.selected.is_empty() {
        finalize_job_run(
            db,
            job_run_id,
            FinishEngineJobRunRequest {
                status: "done".to_string(),
                input_count: 0,
                output_count: 0,
                processed_count: 0,
                success_count: 0,
                error_count: 0,
                metadata: Some(noop_metadata(prep.pending_before_count)),
                error_message: None,
            },
        )
        .await;
        return Ok(noop_response(thread_id, Some(job_run_id)));
    }

    let mut processed_count = 0_i64;
    let output_count = 0_i64;

    let result: Result<RunThreadRepairSummaryResponse, String> = async {
        info!(
            "[MEMORY-ENGINE-REPAIR] build-start thread_id={} job_run_id={} selected_count={} selected_token_count={}",
            thread_id,
            job_run_id,
            prep.selection.selected.len(),
            prep.selection.selected_token_count
        );

        let summary_build = match build_repair_summary_text(
            config,
            db,
            Some(tenant_id),
            prep.thread.title.as_deref(),
            prep.selection.selected.as_slice(),
            &prep.settings,
            Some(job_run_id),
        )
        .await
        {
            Ok(build) => {
                info!(
                    "[MEMORY-ENGINE-REPAIR] build-done thread_id={} job_run_id={} chunk_count={} overflow_retry_count={} summary_chars={}",
                    thread_id,
                    job_run_id,
                    build.chunk_count,
                    build.overflow_retry_count,
                    build.text.chars().count()
                );
                build
            }
            Err(err) => {
                warn!(
                    "[MEMORY-ENGINE-REPAIR] build-failed thread_id={} job_run_id={} selected_count={} selected_token_count={} error={}",
                    thread_id,
                    job_run_id,
                    prep.selection.selected.len(),
                    prep.selection.selected_token_count,
                    err
                );
                return Err(err);
            }
        };
        let summary_text = decorate_generated_text(summary_build, None, "thread repair summary");
        let summary = summaries::create_thread_summary_with_type(
            db,
            tenant_id,
            source_id,
            thread_id,
            prep.thread.subject_id.as_str(),
            "thread_repair",
            None,
            summary_text.as_str(),
            prep.selection.selected.first().map(|item| item.id.clone()),
            prep.selection.selected.last().map(|item| item.id.clone()),
            prep.selection.selected.len(),
            Some(serde_json::json!({
                "generator": "memory_engine_repair_v1",
                "summary_role": "repair"
            })),
        )
        .await?;
        info!(
            "[MEMORY-ENGINE-REPAIR] summary-created thread_id={} job_run_id={} summary_id={} source_record_count={}",
            thread_id,
            job_run_id,
            summary.id,
            prep.selection.selected.len()
        );
        processed_count = prep.selection.selected.len() as i64;
        let record_ids = prep
            .selection
            .selected
            .iter()
            .map(|item| item.id.clone())
            .collect::<Vec<_>>();
        let marked_messages = match records::mark_records_summarized(
            db,
            tenant_id,
            source_id,
            thread_id,
            record_ids.as_slice(),
            summary.id.as_str(),
        )
        .await
        {
            Ok(marked) => marked,
            Err(err) => {
                warn!(
                    "[MEMORY-ENGINE-REPAIR] mark-records-failed thread_id={} job_run_id={} summary_id={} error={}",
                    thread_id,
                    job_run_id,
                    summary.id,
                    err
                );
                let _ = summaries::delete_thread_summary(
                    db,
                    thread_id,
                    summary.id.as_str(),
                    Some(tenant_id),
                    Some(source_id),
                )
                .await;
                return Err(format!("mark records summarized failed: {}", err));
            }
        };
        info!(
            "[MEMORY-ENGINE-REPAIR] records-marked thread_id={} job_run_id={} summary_id={} marked_count={}",
            thread_id,
            job_run_id,
            summary.id,
            marked_messages
        );
        let pending_after_count = records::count_records(
            db,
            thread_id,
            Some(tenant_id),
            Some(source_id),
            None,
            None,
            Some("pending"),
        )
        .await?;
        finalize_job_run(
            db,
            job_run_id,
            FinishEngineJobRunRequest {
                status: "done".to_string(),
                input_count: prep.selection.selected.len() as i64,
                output_count: 1,
                processed_count: prep.selection.selected.len() as i64,
                success_count: prep.selection.selected.len() as i64,
                error_count: 0,
                metadata: Some(success_metadata(
                    prep.pending_before_count,
                    prep.selection.selected.len(),
                    prep.selection.selected_token_count,
                    marked_messages,
                    pending_after_count,
                    summary.id.as_str(),
                )),
                error_message: None,
            },
        )
        .await;
        let _ = threads::refresh_summary_queue_state(
            db,
            tenant_id,
            source_id,
            thread_id,
        )
        .await;
        info!(
            "[MEMORY-ENGINE-REPAIR] completed thread_id={} job_run_id={} summary_id={} pending_after_count={} processed_count={}",
            thread_id,
            job_run_id,
            summary.id,
            pending_after_count,
            prep.selection.selected.len()
        );

        Ok(success_response(
            thread_id,
            job_run_id,
            summary.id,
            prep.selection.selected.len(),
        ))
    }
    .await;

    if let Err(err) = &result {
        warn!(
            "[MEMORY-ENGINE-REPAIR] result-failed thread_id={} job_run_id={} pending_before_count={} processed_count={} output_count={} error={}",
            thread_id,
            job_run_id,
            prep.pending_before_count,
            processed_count,
            output_count,
            err
        );
        finalize_job_run(
            db,
            job_run_id,
            FinishEngineJobRunRequest {
                status: "failed".to_string(),
                input_count: prep.pending_before_count.max(0),
                output_count,
                processed_count,
                success_count: output_count,
                error_count: 1,
                metadata: Some(failed_metadata(
                    prep.pending_before_count,
                    processed_count,
                    output_count,
                )),
                error_message: Some(err.clone()),
            },
        )
        .await;
    }

    result
}
