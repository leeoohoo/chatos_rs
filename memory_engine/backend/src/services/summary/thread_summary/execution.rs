// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::config::AppConfig;
use crate::db::Db;
use crate::models::{
    EngineJobRun, EngineThread, FinishEngineJobRunRequest, RunThreadSummaryResponse,
};
use crate::repositories::{
    control_plane as cp_repo, generation_commits, generation_output_commits, records,
    thread_partitions, threads,
};

use super::super::builders::{
    build_summary_text, THREAD_SUMMARY_GENERATION_PROFILE, THREAD_SUMMARY_PIPELINE_REVISION,
};
use super::super::selectors::select_pending_records_for_summary;
use super::super::settings::load_summary_job_settings;
use super::super::{PendingRecordSelection, SummaryJobSettings, DEFAULT_PENDING_RECORD_SCAN_LIMIT};
use super::job::{
    create_thread_summary_job_run, done_metadata, failed_metadata, finish_thread_summary_job_run,
    noop_metadata, FrozenThreadSummarySelection, THREAD_DIRECT_TRIGGER,
};

const PARTITION_CLAIM_LEASE_SECONDS: i64 = 300;
const GENERATION_FINALIZE_LEASE_SECONDS: i64 = 300;
const THREAD_SUMMARY_GENERATION_STEP: &str = "thread_summary_final";

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
    requested_partition: Option<&str>,
) -> Result<ThreadSummaryExecutionContext, String> {
    let thread_id = thread.id.clone();
    let tenant_id = thread.tenant_id.clone();
    let source_id = thread.source_id.clone();
    let pending_records = if let Some(partition) = requested_partition {
        records::list_pending_records_for_partition(
            db,
            tenant_id.as_str(),
            source_id.as_str(),
            thread_id.as_str(),
            partition,
            DEFAULT_PENDING_RECORD_SCAN_LIMIT,
        )
        .await?
    } else {
        records::list_pending_records(
            db,
            tenant_id.as_str(),
            source_id.as_str(),
            thread_id.as_str(),
            DEFAULT_PENDING_RECORD_SCAN_LIMIT,
        )
        .await?
    };
    let pending_before_count = pending_records.len() as i64;
    let selection = select_pending_records_for_summary(
        pending_records,
        settings.source_batch_token_limit,
    );

    Ok(ThreadSummaryExecutionContext {
        thread,
        settings,
        pending_before_count,
        selection,
    })
}

pub(crate) async fn run_thread_summary_partition_with_thread(
    config: &AppConfig,
    db: &Db,
    thread: EngineThread,
    requested_partition: &str,
    trigger_type: &str,
) -> Result<RunThreadSummaryResponse, String> {
    let settings = load_summary_job_settings(db, "summary").await?;
    let mut ctx =
        build_thread_summary_execution_context(db, thread, settings, Some(requested_partition))
            .await?;
    ctx.settings.cloud_resume_kind =
        Some(crate::services::memory_cloud_agent::THREAD_SUMMARY_JOB_RESUME_KIND.to_string());
    if !ctx.should_run() {
        return Ok(noop_response(ctx.thread.id.as_str()));
    }
    if project_partition_key(&ctx.selection) != requested_partition {
        return Err("partition summary selection changed project identity".to_string());
    }
    let tenant_id = ctx.thread.tenant_id.clone();
    let source_id = ctx.thread.source_id.clone();
    let thread_id = ctx.thread.id.clone();
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

pub async fn run_thread_summary(
    config: &AppConfig,
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    thread_id: &str,
    project_partition_key: &str,
) -> Result<RunThreadSummaryResponse, String> {
    let thread = threads::get_thread_by_id(db, tenant_id, source_id, thread_id)
        .await?
        .ok_or_else(|| "thread not found".to_string())?;
    run_thread_summary_partition_with_thread(
        config,
        db,
        thread,
        project_partition_key,
        THREAD_DIRECT_TRIGGER,
    )
    .await
}

pub(crate) async fn load_thread_summary_execution_context_for_partition(
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    thread_id: &str,
    project_partition_key: &str,
) -> Result<ThreadSummaryExecutionContext, String> {
    let thread = threads::get_thread_by_id(db, tenant_id, source_id, thread_id)
        .await?
        .ok_or_else(|| "thread not found".to_string())?;
    let settings = load_summary_job_settings(db, "summary").await?;
    build_thread_summary_execution_context(db, thread, settings, Some(project_partition_key)).await
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
        &ctx.settings,
        trigger_type,
        &ctx.selection,
    )
    .await?;

    let project_partition_key = project_partition_key(&ctx.selection);
    let Some(_claim) = thread_partitions::try_claim_summary(
        db,
        tenant_id,
        source_id,
        thread_id,
        project_partition_key,
        job_run.id.as_str(),
        PARTITION_CLAIM_LEASE_SECONDS,
    )
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
            let _ = thread_partitions::release_summary_claim(
                db,
                tenant_id,
                source_id,
                thread_id,
                project_partition_key,
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
        let _ = thread_partitions::release_summary_claim(
            db,
            tenant_id,
            source_id,
            thread_id,
            project_partition_key,
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
    ctx: ThreadSummaryExecutionContext,
) -> Result<RunThreadSummaryResponse, String> {
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
            token_limit: metadata_i64(job_run.metadata.as_ref(), "policy_max_model_input_tokens")
                .ok_or_else(|| "summary job metadata policy_max_model_input_tokens is missing".to_string())?,
            source_batch_token_limit: metadata_i64(
                job_run.metadata.as_ref(),
                "policy_max_source_batch_tokens",
            )
            .ok_or_else(|| "summary job metadata policy_max_source_batch_tokens is missing".to_string())?,
            max_leaf_tokens: metadata_i64(job_run.metadata.as_ref(), "policy_max_leaf_tokens")
                .ok_or_else(|| "summary job metadata policy_max_leaf_tokens is missing".to_string())?,
            max_merge_rounds: metadata_i64(job_run.metadata.as_ref(), "policy_max_merge_rounds")
                .ok_or_else(|| "summary job metadata policy_max_merge_rounds is missing".to_string())?
                .max(1) as usize,
            cloud_owner_entity_id: Some(job_run_id.to_string()),
            cloud_resume_kind: Some(
                crate::services::memory_cloud_agent::THREAD_SUMMARY_JOB_RESUME_KIND.to_string(),
            ),
        },
        pending_before_count: metadata_i64(job_run.metadata.as_ref(), "pending_before_count")
            .unwrap_or_else(|| (selected.len() + oversized.len()) as i64),
        selection: PendingRecordSelection {
            selected,
            oversized,
            selected_token_count: frozen.selected_token_count,
            oversized_token_count: frozen.oversized_token_count,
            project_subject_id: frozen.project_subject_id,
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
) -> Result<bool, String> {
    let Some(job_run) = cp_repo::get_job_run_by_id(db, job_run_id).await? else {
        return Ok(false);
    };
    validate_summary_job_scope(&job_run, tenant_id, source_id, thread_id)?;
    if job_run.status != "running" {
        return Ok(false);
    }

    let frozen = FrozenThreadSummarySelection::from_metadata(job_run.metadata.as_ref())?;
    let pending_before_count = metadata_i64(job_run.metadata.as_ref(), "pending_before_count")
        .unwrap_or_else(|| {
            (frozen.selected_record_ids.len() + frozen.oversized_record_ids.len()) as i64
        });
    let skipped_count = frozen.oversized_record_ids.len();
    let input_count = frozen.selected_record_ids.len() + skipped_count;
    let _ = records::release_records_from_summary(db, tenant_id, source_id, thread_id, job_run_id)
        .await;
    finish_thread_summary_job_run(
        db,
        job_run_id,
        FinishEngineJobRunRequest {
            status: "failed".to_string(),
            input_count: input_count as i64,
            output_count: 0,
            processed_count: 0,
            success_count: 0,
            error_count: input_count as i64,
            metadata: Some(failed_metadata(
                pending_before_count,
                Some(frozen.selected_record_ids.len()),
                Some(frozen.selected_token_count),
                skipped_count,
                Some(pending_before_count),
                0,
                0,
            )),
            error_message: Some(error),
        },
    )
    .await;
    thread_partitions::fail_owned_summary_claim_and_dead_letter_dispatch(
        db,
        tenant_id,
        source_id,
        thread_id,
        project_partition_key_from_subject(frozen.project_subject_id.as_deref()),
        job_run_id,
        "cloud_summary_terminal_failure",
    )
    .await
}

pub(super) fn project_partition_key(selection: &PendingRecordSelection) -> &str {
    project_partition_key_from_subject(selection.project_subject_id.as_deref())
}

fn project_partition_key_from_subject(project_subject_id: Option<&str>) -> &str {
    project_subject_id.unwrap_or("__none__")
}

include!("execution_part01.rs");

fn noop_response(thread_id: &str) -> RunThreadSummaryResponse {
    RunThreadSummaryResponse {
        thread_id: thread_id.to_string(),
        generated: false,
        summary_id: None,
        source_record_count: 0,
    }
}

fn validate_summary_job_scope(
    job_run: &EngineJobRun,
    tenant_id: &str,
    source_id: &str,
    thread_id: &str,
) -> Result<(), String> {
    if job_run.job_type != "summary"
        || job_run.tenant_id.as_deref() != Some(tenant_id)
        || job_run.source_id.as_deref() != Some(source_id)
        || job_run.thread_id.as_deref() != Some(thread_id)
    {
        return Err("summary job scope does not match its Cloud Agent callback".to_string());
    }
    Ok(())
}

fn completed_job_response(thread_id: &str, job_run: &EngineJobRun) -> RunThreadSummaryResponse {
    let summary_id = job_run
        .metadata
        .as_ref()
        .and_then(|value| value.get("generated_summary_id"))
        .and_then(|value| value.as_str())
        .map(ToOwned::to_owned);
    RunThreadSummaryResponse {
        thread_id: thread_id.to_string(),
        generated: summary_id.is_some(),
        summary_id,
        source_record_count: metadata_i64(job_run.metadata.as_ref(), "selected_count")
            .unwrap_or(0)
            .max(0) as usize,
    }
}

fn metadata_i64(metadata: Option<&serde_json::Value>, key: &str) -> Option<i64> {
    metadata
        .and_then(|value| value.get(key))
        .and_then(|value| value.as_i64())
}
