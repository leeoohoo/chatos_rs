// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::config::AppConfig;
use crate::db::Db;
use crate::models::{
    EngineJobRun, EngineThread, FinishEngineJobRunRequest, RunThreadSummaryResponse,
};
use crate::repositories::{control_plane as cp_repo, records, summaries, threads};

use super::super::builders::build_summary_text;
use super::super::render::decorate_generated_text;
use super::super::selectors::{
    mark_oversized_records_as_summarized, select_pending_records_for_summary,
};
use super::super::settings::load_summary_job_settings;
use super::super::{
    PendingRecordSelection, SummaryJobSettings, DEFAULT_PENDING_RECORD_SCAN_LIMIT,
    MAX_THREAD_SUMMARY_TARGET_TOKENS,
};
use super::job::{
    create_thread_summary_job_run, done_metadata, failed_metadata, finish_thread_summary_job_run,
    noop_metadata, FrozenThreadSummarySelection, THREAD_DIRECT_TRIGGER,
};

#[derive(Debug, Clone)]
pub(crate) struct ThreadSummaryExecutionContext {
    pub(crate) thread: EngineThread,
    pub(crate) settings: SummaryJobSettings,
    pub(crate) pending_before_count: i64,
    pub(crate) selection: PendingRecordSelection,
}

impl ThreadSummaryExecutionContext {
    pub(crate) fn should_run(&self) -> bool {
        !self.selection.selected.is_empty() || !self.selection.oversized.is_empty()
    }
}

async fn build_thread_summary_execution_context(
    db: &Db,
    thread: EngineThread,
    settings: SummaryJobSettings,
) -> Result<ThreadSummaryExecutionContext, String> {
    let now = crate::models::now_rfc3339();
    let summary_lock_is_active = thread.summary_status == "running"
        && thread
            .summary_lock_expires_at
            .as_deref()
            .is_some_and(|expires_at| expires_at > now.as_str());
    if thread.summary_status == "running" && !summary_lock_is_active {
        if let Some(stale_job_run_id) = thread.summary_job_run_id.as_deref() {
            records::release_records_from_summary(
                db,
                thread.tenant_id.as_str(),
                thread.source_id.as_str(),
                thread.id.as_str(),
                stale_job_run_id,
            )
            .await?;
        }
    }
    let thread_id = thread.id.clone();
    let tenant_id = thread.tenant_id.clone();
    let source_id = thread.source_id.clone();
    let pending_before_count = records::count_records(
        db,
        thread_id.as_str(),
        Some(tenant_id.as_str()),
        Some(source_id.as_str()),
        None,
        None,
        Some("pending"),
    )
    .await?;
    let pending_records = records::list_pending_records(
        db,
        tenant_id.as_str(),
        source_id.as_str(),
        thread_id.as_str(),
        DEFAULT_PENDING_RECORD_SCAN_LIMIT,
    )
    .await?;
    let selection = select_pending_records_for_summary(pending_records, settings.token_limit);

    Ok(ThreadSummaryExecutionContext {
        thread,
        settings,
        pending_before_count,
        selection,
    })
}

pub(crate) async fn load_thread_summary_execution_context_for_thread(
    db: &Db,
    thread: EngineThread,
) -> Result<ThreadSummaryExecutionContext, String> {
    let settings = load_summary_job_settings(db, "summary").await?;
    build_thread_summary_execution_context(db, thread, settings).await
}

pub async fn run_thread_summary(
    config: &AppConfig,
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    thread_id: &str,
) -> Result<RunThreadSummaryResponse, String> {
    let thread = threads::get_thread_by_id(db, tenant_id, source_id, thread_id)
        .await?
        .ok_or_else(|| "thread not found".to_string())?;
    run_thread_summary_with_thread(config, db, thread, THREAD_DIRECT_TRIGGER).await
}

pub(crate) async fn run_thread_summary_with_thread(
    config: &AppConfig,
    db: &Db,
    thread: EngineThread,
    trigger_type: &str,
) -> Result<RunThreadSummaryResponse, String> {
    let tenant_id = thread.tenant_id.clone();
    let source_id = thread.source_id.clone();
    let thread_id = thread.id.clone();
    let mut ctx = load_thread_summary_execution_context_for_thread(db, thread).await?;
    ctx.settings.cloud_resume_kind = Some(trigger_type.to_string());
    if !ctx.should_run() {
        return Ok(noop_response(thread_id.as_str()));
    }

    let job_run = start_thread_summary_job(
        db,
        tenant_id.as_str(),
        source_id.as_str(),
        thread_id.as_str(),
        &ctx,
        trigger_type,
    )
    .await?;
    execute_prepared_thread_summary_job(
        config,
        db,
        tenant_id.as_str(),
        source_id.as_str(),
        thread_id.as_str(),
        job_run.id.as_str(),
        ctx,
    )
    .await
}

pub(crate) async fn load_thread_summary_execution_context(
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    thread_id: &str,
) -> Result<ThreadSummaryExecutionContext, String> {
    let thread = threads::get_thread_by_id(db, tenant_id, source_id, thread_id)
        .await?
        .ok_or_else(|| "thread not found".to_string())?;
    load_thread_summary_execution_context_for_thread(db, thread).await
}

pub(crate) async fn start_thread_summary_job(
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    thread_id: &str,
    ctx: &ThreadSummaryExecutionContext,
    trigger_type: &str,
) -> Result<EngineJobRun, String> {
    let job_run = create_thread_summary_job_run(
        db,
        tenant_id,
        source_id,
        thread_id,
        ctx.thread.subject_id.as_str(),
        ctx.pending_before_count,
        ctx.settings.token_limit,
        ctx.settings.target_summary_tokens.ok_or_else(|| {
            "summary job policy target_summary_tokens is not configured".to_string()
        })?,
        trigger_type,
        &ctx.selection,
    )
    .await?;

    let Some(_locked_thread) =
        threads::try_acquire_summary_slot(db, tenant_id, source_id, thread_id, job_run.id.as_str())
            .await?
    else {
        let _ = finish_thread_summary_job_run(
            db,
            job_run.id.as_str(),
            FinishEngineJobRunRequest {
                status: "failed".to_string(),
                input_count: 0,
                output_count: 0,
                processed_count: 0,
                success_count: 0,
                error_count: 1,
                metadata: Some(failed_metadata(
                    ctx.pending_before_count,
                    None,
                    None,
                    0,
                    None,
                    0,
                    0,
                )),
                error_message: Some("thread summary slot already occupied".to_string()),
            },
        )
        .await;
        return Err("thread summary slot already occupied".to_string());
    };

    let claimed_record_ids = ctx
        .selection
        .selected
        .iter()
        .chain(ctx.selection.oversized.iter())
        .map(|record| record.id.clone())
        .collect::<Vec<_>>();
    let claimed_count = match records::claim_records_for_summary(
        db,
        tenant_id,
        source_id,
        thread_id,
        claimed_record_ids.as_slice(),
        job_run.id.as_str(),
    )
    .await
    {
        Ok(count) => count,
        Err(error) => {
            let _ = threads::release_summary_slot(
                db,
                tenant_id,
                source_id,
                thread_id,
                job_run.id.as_str(),
                0,
                0,
            )
            .await;
            let _ = finish_thread_summary_job_run(
                db,
                job_run.id.as_str(),
                FinishEngineJobRunRequest {
                    status: "failed".to_string(),
                    input_count: claimed_record_ids.len() as i64,
                    output_count: 0,
                    processed_count: 0,
                    success_count: 0,
                    error_count: claimed_record_ids.len() as i64,
                    metadata: Some(failed_metadata(
                        ctx.pending_before_count,
                        Some(ctx.selection.selected.len()),
                        Some(ctx.selection.selected_token_count),
                        ctx.selection.oversized.len(),
                        None,
                        0,
                        0,
                    )),
                    error_message: Some(format!("claim summary records failed: {error}")),
                },
            )
            .await;
            return Err(error);
        }
    };
    if claimed_count != claimed_record_ids.len() {
        let _ = records::release_records_from_summary(
            db,
            tenant_id,
            source_id,
            thread_id,
            job_run.id.as_str(),
        )
        .await;
        let _ = threads::release_summary_slot(
            db,
            tenant_id,
            source_id,
            thread_id,
            job_run.id.as_str(),
            0,
            0,
        )
        .await;
        let error = format!(
            "summary record claim mismatch: expected {}, claimed {}",
            claimed_record_ids.len(),
            claimed_count
        );
        let _ = finish_thread_summary_job_run(
            db,
            job_run.id.as_str(),
            FinishEngineJobRunRequest {
                status: "failed".to_string(),
                input_count: claimed_record_ids.len() as i64,
                output_count: 0,
                processed_count: claimed_count as i64,
                success_count: 0,
                error_count: claimed_record_ids.len().saturating_sub(claimed_count) as i64,
                metadata: Some(failed_metadata(
                    ctx.pending_before_count,
                    Some(ctx.selection.selected.len()),
                    Some(ctx.selection.selected_token_count),
                    ctx.selection.oversized.len(),
                    None,
                    claimed_count as i64,
                    0,
                )),
                error_message: Some(error.clone()),
            },
        )
        .await;
        return Err(error);
    }

    Ok(job_run)
}

pub(crate) async fn execute_existing_summary_job(
    config: &AppConfig,
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    thread_id: &str,
    job_run_id: &str,
    seed_ctx: Option<ThreadSummaryExecutionContext>,
) -> Result<RunThreadSummaryResponse, String> {
    let ctx = if let Some(seed_ctx) = seed_ctx {
        seed_ctx
    } else {
        load_thread_summary_execution_context(db, tenant_id, source_id, thread_id).await?
    };
    execute_prepared_thread_summary_job(
        config, db, tenant_id, source_id, thread_id, job_run_id, ctx,
    )
    .await
}

pub(crate) async fn resume_cloud_summary_job(
    config: &AppConfig,
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    thread_id: &str,
    job_run_id: &str,
) -> Result<RunThreadSummaryResponse, String> {
    let job_run = cp_repo::get_job_run_by_id(db, job_run_id)
        .await?
        .ok_or_else(|| format!("summary job run not found: {job_run_id}"))?;
    validate_summary_job_scope(&job_run, tenant_id, source_id, thread_id)?;
    if job_run.status == "done" {
        return Ok(completed_job_response(thread_id, &job_run));
    }
    if job_run.status != "running" {
        return Err(job_run
            .error_message
            .clone()
            .unwrap_or_else(|| format!("summary job is not resumable: {}", job_run.status)));
    }

    let thread = threads::get_thread_by_id(db, tenant_id, source_id, thread_id)
        .await?
        .ok_or_else(|| "thread not found".to_string())?;
    let frozen = FrozenThreadSummarySelection::from_metadata(job_run.metadata.as_ref())?;
    let selected = records::list_records_by_ids(
        db,
        tenant_id,
        source_id,
        thread_id,
        frozen.selected_record_ids.as_slice(),
    )
    .await?;
    let oversized = records::list_records_by_ids(
        db,
        tenant_id,
        source_id,
        thread_id,
        frozen.oversized_record_ids.as_slice(),
    )
    .await?;
    let ctx = ThreadSummaryExecutionContext {
        thread,
        settings: SummaryJobSettings {
            token_limit: metadata_i64(job_run.metadata.as_ref(), "policy_token_limit")
                .ok_or_else(|| "summary job metadata policy_token_limit is missing".to_string())?,
            target_summary_tokens: Some(
                metadata_i64(job_run.metadata.as_ref(), "policy_target_summary_tokens")
                    .ok_or_else(|| {
                        "summary job metadata policy_target_summary_tokens is missing".to_string()
                    })?
                    .clamp(128, MAX_THREAD_SUMMARY_TARGET_TOKENS),
            ),
            cloud_owner_entity_id: Some(job_run_id.to_string()),
            cloud_resume_kind: Some(job_run.trigger_type.clone()),
        },
        pending_before_count: metadata_i64(job_run.metadata.as_ref(), "pending_before_count")
            .unwrap_or_else(|| (selected.len() + oversized.len()) as i64),
        selection: PendingRecordSelection {
            selected,
            oversized,
            selected_token_count: frozen.selected_token_count,
            oversized_token_count: frozen.oversized_token_count,
        },
    };
    execute_prepared_thread_summary_job(
        config, db, tenant_id, source_id, thread_id, job_run_id, ctx,
    )
    .await
}

pub(crate) async fn fail_cloud_summary_job(
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    thread_id: &str,
    job_run_id: &str,
    error: String,
) -> Result<(), String> {
    let Some(job_run) = cp_repo::get_job_run_by_id(db, job_run_id).await? else {
        return Ok(());
    };
    validate_summary_job_scope(&job_run, tenant_id, source_id, thread_id)?;
    if job_run.status != "running" {
        return Ok(());
    }

    let frozen = FrozenThreadSummarySelection::from_metadata(job_run.metadata.as_ref())?;
    let pending_before_count = metadata_i64(job_run.metadata.as_ref(), "pending_before_count")
        .unwrap_or_else(|| {
            (frozen.selected_record_ids.len() + frozen.oversized_record_ids.len()) as i64
        });
    let skipped_count = frozen.oversized_record_ids.len();
    let _ = records::release_records_from_summary(db, tenant_id, source_id, thread_id, job_run_id)
        .await;
    finish_thread_summary_job_run(
        db,
        job_run_id,
        FinishEngineJobRunRequest {
            status: "failed".to_string(),
            input_count: frozen.selected_record_ids.len() as i64,
            output_count: 0,
            processed_count: skipped_count as i64,
            success_count: skipped_count as i64,
            error_count: frozen.selected_record_ids.len() as i64,
            metadata: Some(failed_metadata(
                pending_before_count,
                Some(frozen.selected_record_ids.len()),
                Some(frozen.selected_token_count),
                skipped_count,
                Some(pending_before_count.saturating_sub(skipped_count as i64)),
                skipped_count as i64,
                0,
            )),
            error_message: Some(error),
        },
    )
    .await;
    threads::release_summary_slot(
        db,
        tenant_id,
        source_id,
        thread_id,
        job_run_id,
        skipped_count as i64,
        frozen.oversized_token_count,
    )
    .await?;
    Ok(())
}

include!("execution_part01.rs");
