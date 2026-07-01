// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::config::AppConfig;
use crate::db::Db;
use crate::models::{
    EngineJobRun, EngineThread, FinishEngineJobRunRequest, RunThreadSummaryResponse,
};
use crate::repositories::{records, summaries, threads};

use super::super::builders::build_summary_text;
use super::super::render::decorate_generated_text;
use super::super::selectors::{
    mark_oversized_records_as_summarized, select_pending_records_for_summary,
};
use super::super::settings::load_summary_job_settings;
use super::super::{
    PendingRecordSelection, SummaryJobSettings, DEFAULT_PENDING_RECORD_SCAN_LIMIT,
    DEFAULT_ROLLUP_TARGET_TOKENS,
};
use super::job::{
    create_thread_summary_job_run, done_metadata, failed_metadata, finish_thread_summary_job_run,
    noop_metadata, THREAD_DIRECT_TRIGGER,
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
    let ctx = load_thread_summary_execution_context_for_thread(db, thread).await?;
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
        ctx.settings
            .target_summary_tokens
            .unwrap_or(DEFAULT_ROLLUP_TARGET_TOKENS),
        trigger_type,
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
        let ThreadSummaryExecutionContext {
            thread, settings, ..
        } = seed_ctx;
        build_thread_summary_execution_context(db, thread, settings).await?
    } else {
        load_thread_summary_execution_context(db, tenant_id, source_id, thread_id).await?
    };
    execute_prepared_thread_summary_job(
        config, db, tenant_id, source_id, thread_id, job_run_id, ctx,
    )
    .await
}

pub(crate) async fn execute_prepared_thread_summary_job(
    config: &AppConfig,
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    thread_id: &str,
    job_run_id: &str,
    ctx: ThreadSummaryExecutionContext,
) -> Result<RunThreadSummaryResponse, String> {
    let ThreadSummaryExecutionContext {
        thread,
        settings,
        pending_before_count,
        selection,
    } = ctx;

    let mut processed_count = 0_i64;
    let output_count = 0_i64;

    let result: Result<RunThreadSummaryResponse, String> = async {
        let skipped_pending_count = selection.oversized.len() as i64;
        let selected_pending_tokens = selection.selected_token_count.max(0);
        let skipped_pending_tokens = selection.oversized_token_count.max(0);
        let pending_after_skip_count = pending_before_count.saturating_sub(skipped_pending_count);
        let pending_after_skip_tokens = thread
            .pending_summary_tokens
            .max(0)
            .saturating_sub(skipped_pending_tokens);
        let skipped_count = mark_oversized_records_as_summarized(
            db,
            tenant_id,
            source_id,
            thread_id,
            selection.oversized.as_slice(),
            "skipped_single_record_token_limit",
        )
        .await?;
        let skipped_count_i64 = skipped_count as i64;

        if selection.selected.is_empty() {
            finish_thread_summary_job_run(
                db,
                job_run_id,
                FinishEngineJobRunRequest {
                    status: "done".to_string(),
                    input_count: 0,
                    output_count: 0,
                    processed_count: 0,
                    success_count: 0,
                    error_count: 0,
                    metadata: Some(noop_metadata(
                        pending_before_count,
                        pending_before_count.saturating_sub(skipped_count_i64),
                        skipped_count,
                    )),
                    error_message: None,
                },
            )
            .await;
            let _ = threads::release_summary_slot(
                db,
                tenant_id,
                source_id,
                thread_id,
                job_run_id,
                if pending_after_skip_count > 0 {
                    "pending"
                } else {
                    "idle"
                },
                Some(pending_before_count.saturating_sub(skipped_count_i64)),
                Some(pending_after_skip_tokens),
            )
            .await;
            return Ok(noop_response(thread_id));
        }

        let summary_build = match build_summary_text(
            config,
            db,
            Some(tenant_id),
            thread.title.as_deref(),
            selection.selected.as_slice(),
            &settings,
        )
        .await
        {
            Ok(build) => build,
            Err(err) => {
                finish_thread_summary_job_run(
                    db,
                    job_run_id,
                    FinishEngineJobRunRequest {
                        status: "failed".to_string(),
                        input_count: selection.selected.len() as i64,
                        output_count: 0,
                        processed_count: selection.selected.len() as i64 + skipped_count as i64,
                        success_count: 0,
                        error_count: selection.selected.len() as i64,
                        metadata: Some(failed_metadata(
                            pending_before_count,
                            Some(selection.selected.len()),
                            Some(selection.selected_token_count),
                            skipped_count,
                            Some(pending_before_count.saturating_sub(skipped_count_i64)),
                            selection.selected.len() as i64 + skipped_count as i64,
                            0,
                        )),
                        error_message: Some(err.clone()),
                    },
                )
                .await;
                let _ = threads::release_summary_slot(
                    db,
                    tenant_id,
                    source_id,
                    thread_id,
                    job_run_id,
                    if pending_after_skip_count > 0 {
                        "pending"
                    } else {
                        "idle"
                    },
                    Some(pending_before_count.saturating_sub(skipped_count_i64)),
                    Some(pending_after_skip_tokens),
                )
                .await;
                return Err(err);
            }
        };
        let summary_text =
            decorate_generated_text(summary_build, Some(skipped_count), "message summary");
        let summary = summaries::create_thread_summary(
            db,
            tenant_id,
            source_id,
            thread_id,
            thread.subject_id.as_str(),
            summary_text.as_str(),
            selection.selected.first().map(|item| item.id.clone()),
            selection.selected.last().map(|item| item.id.clone()),
            selection.selected.len(),
        )
        .await?;
        processed_count = selection.selected.len() as i64 + skipped_count as i64;

        let record_ids = selection
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
                let _ = summaries::delete_thread_summary(
                    db,
                    thread_id,
                    summary.id.as_str(),
                    Some(tenant_id),
                    Some(source_id),
                )
                .await;
                finish_thread_summary_job_run(
                    db,
                    job_run_id,
                    FinishEngineJobRunRequest {
                        status: "failed".to_string(),
                        input_count: selection.selected.len() as i64,
                        output_count: 0,
                        processed_count: selection.selected.len() as i64 + skipped_count as i64,
                        success_count: 0,
                        error_count: selection.selected.len() as i64,
                        metadata: Some(failed_metadata(
                            pending_before_count,
                            Some(selection.selected.len()),
                            Some(selection.selected_token_count),
                            skipped_count,
                            Some(pending_before_count.saturating_sub(skipped_count_i64)),
                            selection.selected.len() as i64 + skipped_count as i64,
                            0,
                        )),
                        error_message: Some(format!("mark records summarized failed: {}", err)),
                    },
                )
                .await;
                let _ = threads::release_summary_slot(
                    db,
                    tenant_id,
                    source_id,
                    thread_id,
                    job_run_id,
                    if pending_after_skip_count > 0 {
                        "pending"
                    } else {
                        "idle"
                    },
                    Some(pending_before_count.saturating_sub(skipped_count_i64)),
                    Some(pending_after_skip_tokens),
                )
                .await;
                return Err(err);
            }
        };
        let pending_after_count = pending_before_count
            .saturating_sub(skipped_count_i64)
            .saturating_sub(marked_messages as i64);
        let pending_after_tokens = thread
            .pending_summary_tokens
            .max(0)
            .saturating_sub(skipped_pending_tokens)
            .saturating_sub(selected_pending_tokens);
        finish_thread_summary_job_run(
            db,
            job_run_id,
            FinishEngineJobRunRequest {
                status: "done".to_string(),
                input_count: selection.selected.len() as i64,
                output_count: 1,
                processed_count: selection.selected.len() as i64 + skipped_count as i64,
                success_count: selection.selected.len() as i64 + skipped_count as i64,
                error_count: 0,
                metadata: Some(done_metadata(
                    pending_before_count,
                    selection.selected.len(),
                    selection.selected_token_count,
                    marked_messages + skipped_count,
                    pending_after_count,
                    skipped_count,
                    summary.id.as_str(),
                )),
                error_message: None,
            },
        )
        .await;
        let _ = threads::release_summary_slot(
            db,
            tenant_id,
            source_id,
            thread_id,
            job_run_id,
            if pending_after_count > 0 {
                "pending"
            } else {
                "idle"
            },
            Some(pending_after_count),
            Some(pending_after_tokens),
        )
        .await;

        Ok(RunThreadSummaryResponse {
            thread_id: thread_id.to_string(),
            generated: true,
            summary_id: Some(summary.id),
            source_record_count: selection.selected.len(),
        })
    }
    .await;

    if let Err(err) = &result {
        finish_thread_summary_job_run(
            db,
            job_run_id,
            FinishEngineJobRunRequest {
                status: "failed".to_string(),
                input_count: pending_before_count.max(0),
                output_count,
                processed_count,
                success_count: output_count,
                error_count: 1,
                metadata: Some(failed_metadata(
                    pending_before_count,
                    None,
                    None,
                    0,
                    None,
                    processed_count,
                    output_count,
                )),
                error_message: Some(err.clone()),
            },
        )
        .await;
        let _ = threads::release_summary_slot(
            db,
            tenant_id,
            source_id,
            thread_id,
            job_run_id,
            if pending_before_count > 0 {
                "pending"
            } else {
                "idle"
            },
            Some(pending_before_count),
            Some(thread.pending_summary_tokens.max(0)),
        )
        .await;
    }

    result
}

fn noop_response(thread_id: &str) -> RunThreadSummaryResponse {
    RunThreadSummaryResponse {
        thread_id: thread_id.to_string(),
        generated: false,
        summary_id: None,
        source_record_count: 0,
    }
}
