// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use tokio::task;
use tracing::{info, warn};

use crate::config::AppConfig;
use crate::db::Db;
use crate::models::{
    EngineJobRun, GetThreadActiveSummaryStatusRequest, RunThreadActiveSummaryResponse,
};
use crate::repositories::{control_plane as cp_repo, thread_partitions};

use super::super::thread_summary::{
    execute_existing_summary_job, load_thread_summary_execution_context_for_partition,
    start_thread_summary_job, ThreadSummaryExecutionContext, THREAD_DIRECT_TRIGGER,
};

pub async fn run_thread_active_summary(
    config: &AppConfig,
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    thread_id: &str,
    project_partition_key: &str,
    trigger_reason: Option<&str>,
) -> Result<RunThreadActiveSummaryResponse, String> {
    let partition = thread_partitions::get_summary_partition_state(
        db,
        tenant_id,
        source_id,
        thread_id,
        project_partition_key,
    )
    .await?;
    let pending_before_count = partition
        .as_ref()
        .map(|state| state.pending_record_count.max(0))
        .unwrap_or(0);
    if let Some(existing_job) =
        find_running_summary_job(db, tenant_id, source_id, thread_id, project_partition_key).await?
    {
        info!(
            "[MEMORY-ENGINE-ACTIVE-SUMMARY] reuse-running tenant_id={} source_id={} thread_id={} job_run_id={}",
            tenant_id, source_id, thread_id, existing_job.id
        );
        return Ok(build_running_response(
            thread_id,
            Some(existing_job.id),
            pending_before_count,
        ));
    }

    let mut ctx = load_thread_summary_execution_context_for_partition(
        db,
        tenant_id,
        source_id,
        thread_id,
        project_partition_key,
    )
    .await?;
    ctx.settings.cloud_resume_kind =
        Some(crate::services::memory_cloud_agent::THREAD_SUMMARY_JOB_RESUME_KIND.to_string());
    if !ctx.should_run() {
        return Ok(RunThreadActiveSummaryResponse {
            thread_id: thread_id.to_string(),
            accepted: true,
            running: false,
            completed: true,
            failed: false,
            job_run_id: None,
            generated: false,
            summary_id: None,
            source_record_count: 0,
            pending_before_count: Some(ctx.pending_before_count),
            pending_after_count: Some(ctx.pending_before_count),
            compacted: false,
            error_message: None,
        });
    }

    if let Some(existing_job_run_id) = partition
        .as_ref()
        .and_then(|state| state.summary_job_run_id.as_deref())
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        if let Some(existing_job) = cp_repo::get_job_run_by_id(db, existing_job_run_id).await? {
            if existing_job.status == "running" {
                return Ok(build_running_response(
                    thread_id,
                    Some(existing_job.id),
                    ctx.pending_before_count,
                ));
            }
        }
    }

    let job_run = match start_thread_summary_job(
        db,
        tenant_id,
        source_id,
        thread_id,
        &ctx,
        THREAD_DIRECT_TRIGGER,
    )
    .await
    {
        Ok(job_run) => job_run,
        Err(err) if err.contains("slot already occupied") => {
            if let Some(existing_job) =
                find_running_summary_job(db, tenant_id, source_id, thread_id, project_partition_key)
                    .await?
            {
                info!(
                    "[MEMORY-ENGINE-ACTIVE-SUMMARY] reused-raced-running tenant_id={} source_id={} thread_id={} job_run_id={}",
                    tenant_id, source_id, thread_id, existing_job.id
                );
                return Ok(build_running_response(
                    thread_id,
                    Some(existing_job.id),
                    ctx.pending_before_count,
                ));
            }
            return Ok(build_running_response(
                thread_id,
                None,
                ctx.pending_before_count,
            ));
        }
        Err(err) => return Err(err),
    };
    let job_run_id = job_run.id.clone();
    let pending_before_count = ctx.pending_before_count;
    spawn_detached_active_summary_job(
        config,
        db,
        tenant_id,
        source_id,
        thread_id,
        job_run_id.as_str(),
        ctx,
    );

    info!(
        "[MEMORY-ENGINE-ACTIVE-SUMMARY] triggered tenant_id={} source_id={} thread_id={} partition={} job_run_id={} trigger_reason={}",
        tenant_id,
        source_id,
        thread_id,
        project_partition_key,
        job_run_id,
        trigger_reason.unwrap_or("-")
    );

    Ok(build_running_response(
        thread_id,
        Some(job_run_id),
        pending_before_count,
    ))
}

pub async fn get_thread_active_summary_status(
    db: &Db,
    thread_id: &str,
    req: GetThreadActiveSummaryStatusRequest,
) -> Result<RunThreadActiveSummaryResponse, String> {
    let partition = thread_partitions::get_summary_partition_state(
        db,
        req.tenant_id.as_str(),
        req.source_id.as_str(),
        thread_id,
        req.project_partition_key.as_str(),
    )
    .await?;
    let pending_after_count = partition
        .as_ref()
        .map(|state| state.pending_record_count.max(0))
        .unwrap_or(0);

    let job = if let Some(job_run_id) = req
        .job_run_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        cp_repo::get_job_run_by_id(db, job_run_id).await?
    } else if let Some(job_run_id) = partition
        .as_ref()
        .and_then(|state| state.summary_job_run_id.as_deref())
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        cp_repo::get_job_run_by_id(db, job_run_id).await?
    } else {
        find_latest_summary_job(
            db,
            req.tenant_id.as_str(),
            req.source_id.as_str(),
            thread_id,
            req.project_partition_key.as_str(),
        )
        .await?
    };

    let Some(job) = job else {
        return Ok(RunThreadActiveSummaryResponse {
            thread_id: thread_id.to_string(),
            accepted: false,
            running: false,
            completed: false,
            failed: false,
            job_run_id: None,
            generated: false,
            summary_id: None,
            source_record_count: 0,
            pending_before_count: None,
            pending_after_count: Some(pending_after_count),
            compacted: false,
            error_message: None,
        });
    };

    Ok(map_job_run_to_status(thread_id, pending_after_count, job))
}

fn spawn_detached_active_summary_job(
    config: &AppConfig,
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    thread_id: &str,
    job_run_id: &str,
    ctx: ThreadSummaryExecutionContext,
) {
    let config_cloned = config.clone();
    let db_cloned = db.clone();
    let tenant_id_owned = tenant_id.to_string();
    let source_id_owned = source_id.to_string();
    let thread_id_owned = thread_id.to_string();
    let job_run_id_owned = job_run_id.to_string();
    task::spawn(async move {
        if let Err(err) = execute_existing_summary_job(
            &config_cloned,
            &db_cloned,
            tenant_id_owned.as_str(),
            source_id_owned.as_str(),
            thread_id_owned.as_str(),
            job_run_id_owned.as_str(),
            ctx,
        )
        .await
        {
            warn!(
                "[MEMORY-ENGINE-ACTIVE-SUMMARY] detached-run-failed thread_id={} job_run_id={} error={}",
                thread_id_owned,
                job_run_id_owned,
                err
            );
        }
    });
}

async fn find_running_summary_job(
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    thread_id: &str,
    project_partition_key: &str,
) -> Result<Option<EngineJobRun>, String> {
    cp_repo::list_job_runs(
        db,
        Some("summary"),
        None,
        Some(thread_id),
        Some("running"),
        Some(tenant_id),
        Some(source_id),
        10,
    )
    .await
    .map(|items| {
        items.into_iter().find(|job| {
            metadata_str(job.metadata.as_ref(), "project_partition_key").as_deref()
                == Some(project_partition_key)
        })
    })
}

async fn find_latest_summary_job(
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    thread_id: &str,
    project_partition_key: &str,
) -> Result<Option<EngineJobRun>, String> {
    cp_repo::list_job_runs(
        db,
        Some("summary"),
        None,
        Some(thread_id),
        None,
        Some(tenant_id),
        Some(source_id),
        50,
    )
    .await
    .map(|items| {
        items.into_iter().find(|job| {
            metadata_str(job.metadata.as_ref(), "project_partition_key").as_deref()
                == Some(project_partition_key)
        })
    })
}

fn build_running_response(
    thread_id: &str,
    job_run_id: Option<String>,
    pending_before_count: i64,
) -> RunThreadActiveSummaryResponse {
    RunThreadActiveSummaryResponse {
        thread_id: thread_id.to_string(),
        accepted: true,
        running: true,
        completed: false,
        failed: false,
        job_run_id,
        generated: false,
        summary_id: None,
        source_record_count: 0,
        pending_before_count: Some(pending_before_count),
        pending_after_count: None,
        compacted: false,
        error_message: None,
    }
}

fn map_job_run_to_status(
    thread_id: &str,
    pending_after_count: i64,
    job: EngineJobRun,
) -> RunThreadActiveSummaryResponse {
    let pending_before_count = metadata_i64(job.metadata.as_ref(), "pending_before_count");
    let pending_after_from_metadata = metadata_i64(job.metadata.as_ref(), "pending_after_count");
    let summary_id = metadata_str(job.metadata.as_ref(), "generated_summary_id");
    let selected_count = metadata_i64(job.metadata.as_ref(), "selected_count").unwrap_or(0);
    let generated = summary_id.is_some() && job.status == "done";
    let completed = job.status != "running";
    let failed = job.status == "failed";
    let effective_pending_after = if completed {
        Some(pending_after_from_metadata.unwrap_or(pending_after_count))
    } else {
        None
    };
    let compacted = pending_before_count
        .zip(effective_pending_after)
        .map(|(before, after)| after < before)
        .unwrap_or(false);

    RunThreadActiveSummaryResponse {
        thread_id: thread_id.to_string(),
        accepted: true,
        running: job.status == "running",
        completed,
        failed,
        job_run_id: Some(job.id),
        generated,
        summary_id,
        source_record_count: selected_count.max(0) as usize,
        pending_before_count,
        pending_after_count: effective_pending_after,
        compacted,
        error_message: job.error_message,
    }
}

fn metadata_i64(metadata: Option<&serde_json::Value>, key: &str) -> Option<i64> {
    metadata
        .and_then(|value| value.get(key))
        .and_then(|value| value.as_i64())
}

fn metadata_str(metadata: Option<&serde_json::Value>, key: &str) -> Option<String> {
    metadata
        .and_then(|value| value.get(key))
        .and_then(|value| value.as_str())
        .map(ToOwned::to_owned)
}
