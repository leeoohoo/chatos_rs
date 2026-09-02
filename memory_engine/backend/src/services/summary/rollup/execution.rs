// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::config::AppConfig;
use crate::db::Db;
use crate::models::FinishEngineJobRunRequest;
use crate::repositories::{summaries, threads};
use crate::services::ai_pipeline::{estimate_tokens_text, SummaryBuildResult};

use super::super::builders::build_rollup_summary_text;
use super::super::render::{
    build_summary_digest, decorate_generated_text, summary_to_rollup_block,
};
use super::super::selectors::select_rollup_batch;
use super::super::{
    PreparedThreadRollup, RollupSettings, ThreadRollupDrainError, ThreadRollupResult,
};
use super::job::{create_rollup_job_run, done_metadata, failed_metadata, finish_rollup_job_run};

fn empty_rollup_result() -> ThreadRollupResult {
    ThreadRollupResult {
        batches: 0,
        generated: 0,
        marked: 0,
    }
}

pub(crate) async fn run_thread_rollups_until_drained(
    config: &AppConfig,
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    thread_id: &str,
    settings: &RollupSettings,
    trigger_type: &str,
) -> Result<ThreadRollupResult, ThreadRollupDrainError> {
    let prepared = prepare_thread_rollup(db, tenant_id, source_id, thread_id, settings)
        .await
        .map_err(|error| ThreadRollupDrainError {
            batches: 0,
            generated: 0,
            marked: 0,
            error,
        })?;
    let Some(prepared) = prepared else {
        return Ok(empty_rollup_result());
    };
    run_prepared_thread_rollup(
        config,
        db,
        tenant_id,
        source_id,
        thread_id,
        prepared,
        settings,
        trigger_type,
    )
    .await
    .map_err(|error| ThreadRollupDrainError {
        batches: 0,
        generated: 0,
        marked: 0,
        error,
    })
}

pub(crate) async fn prepare_thread_rollup(
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    thread_id: &str,
    settings: &RollupSettings,
) -> Result<Option<PreparedThreadRollup>, String> {
    let selection = select_rollup_batch(
        db,
        tenant_id,
        source_id,
        thread_id,
        settings.token_limit.max(500),
        settings.count_limit.max(0),
        settings.keep_level0_count.max(0),
        settings.max_level.max(1),
    )
    .await?;
    let Some((level, selected, trigger_reason)) = selection else {
        return Ok(None);
    };

    Ok(Some(PreparedThreadRollup {
        thread_id: thread_id.to_string(),
        level,
        selected,
        trigger_reason,
        cloud_job_run_id: None,
    }))
}

pub(crate) async fn run_prepared_thread_rollup(
    config: &AppConfig,
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    thread_id: &str,
    prepared: PreparedThreadRollup,
    settings: &RollupSettings,
    trigger_type: &str,
) -> Result<ThreadRollupResult, String> {
    let thread = threads::get_thread_by_id(db, tenant_id, source_id, thread_id)
        .await?
        .ok_or_else(|| "thread not found".to_string())?;
    let PreparedThreadRollup {
        thread_id: prepared_thread_id,
        level,
        selected,
        trigger_reason,
        cloud_job_run_id,
    } = prepared;
    if prepared_thread_id != thread.id {
        return Err("prepared rollup thread_id mismatch".to_string());
    }
    if selected.is_empty() {
        return Ok(empty_rollup_result());
    };

    let job_run = if let Some(job_run_id) = cloud_job_run_id {
        crate::repositories::control_plane::get_job_run_by_id(db, job_run_id.as_str())
            .await?
            .ok_or_else(|| format!("rollup job run not found: {job_run_id}"))?
    } else {
        create_rollup_job_run(
            db,
            tenant_id,
            source_id,
            thread.id.as_str(),
            thread.subject_id.as_str(),
            trigger_type,
        )
        .await?
    };
    let mut settings = settings.clone();
    settings.cloud_owner_entity_id = Some(job_run.id.clone());
    settings.cloud_source_id = Some(source_id.to_string());
    settings.cloud_thread_id = Some(thread_id.to_string());
    let Some(_locked_thread) = threads::try_acquire_rollup_slot(
        db,
        tenant_id,
        source_id,
        thread.id.as_str(),
        job_run.id.as_str(),
        config.rollup_lock_timeout_secs,
    )
    .await?
    else {
        finish_rollup_job_run(
            db,
            job_run.id.as_str(),
            FinishEngineJobRunRequest {
                status: "failed".to_string(),
                input_count: 0,
                output_count: 0,
                processed_count: 0,
                success_count: 0,
                error_count: 1,
                metadata: Some(failed_metadata(None, 0, 0, 0, Some("slot_occupied"))),
                error_message: Some("thread rollup slot already occupied".to_string()),
            },
        )
        .await;
        return Err("thread rollup slot already occupied".to_string());
    };

    let mut processed_count = 0_i64;
    let output_count = 0_i64;

    let result: Result<ThreadRollupResult, String> = async {
        processed_count = selected.len() as i64;

        let target_level = level + 1;
        let selected_ids = selected.iter().map(|item| item.id.clone()).collect::<Vec<_>>();
        let source_digest =
            build_summary_digest(thread.id.as_str(), level, target_level, selected_ids.as_slice());

        if let Some(existing) = summaries::find_summary_by_source_digest(
            db,
            tenant_id,
            source_id,
            thread.id.as_str(),
            target_level,
            source_digest.as_str(),
        )
        .await?
        {
            let marked = summaries::mark_summaries_rolled_up(
                db,
                tenant_id,
                source_id,
                thread.id.as_str(),
                selected_ids.as_slice(),
                existing.id.as_str(),
            )
            .await?;
            finish_rollup_job_run(
                db,
                job_run.id.as_str(),
                FinishEngineJobRunRequest {
                    status: "done".to_string(),
                    input_count: selected.len() as i64,
                    output_count: 0,
                    processed_count: selected.len() as i64,
                    success_count: marked as i64,
                    error_count: 0,
                    metadata: Some(done_metadata(
                        selected.len(),
                        marked,
                        Some(existing.id.as_str()),
                        trigger_reason,
                    )),
                    error_message: None,
                },
            )
            .await;
            return Ok(ThreadRollupResult {
                batches: 1,
                generated: 0,
                marked,
            });
        }

        let mut summarizable = Vec::new();
        let mut oversized = 0usize;
        for summary in &selected {
            let block = summary_to_rollup_block(summary);
            if estimate_tokens_text(block.as_str()) > settings.token_limit.max(500) {
                oversized += 1;
            } else {
                summarizable.push(block);
            }
        }

        let summary_build = if summarizable.is_empty() {
            SummaryBuildResult {
                text: format!(
                    "All {} selected summaries at level {} exceeded token_limit={}, so this rollup only marks the batch as rolled up.",
                    selected.len(),
                    level,
                    settings.token_limit.max(500)
                ),
                chunk_count: 1,
                overflow_retry_count: 0,
            }
        } else {
            match build_rollup_summary_text(
                config,
                db,
                tenant_id,
                thread.title.as_deref(),
                summarizable.as_slice(),
                &settings,
                level,
                target_level,
            )
            .await
            {
                Ok(build) => build,
                Err(err) => {
                    finish_rollup_job_run(
                        db,
                        job_run.id.as_str(),
                        FinishEngineJobRunRequest {
                            status: "failed".to_string(),
                            input_count: selected.len() as i64,
                            output_count: 0,
                            processed_count: selected.len() as i64,
                            success_count: 0,
                            error_count: selected.len() as i64,
                            metadata: Some(failed_metadata(
                                Some(selected.len()),
                                0,
                                selected.len() as i64,
                                0,
                                Some(trigger_reason),
                            )),
                            error_message: Some(err.clone()),
                        },
                    )
                    .await;
                    return Err(err);
                }
            }
        };
        let summary_text =
            decorate_generated_text(summary_build, Some(oversized), "rollup summary");

        let created = summaries::create_rollup_summary(
            db,
            tenant_id,
            source_id,
            thread.id.as_str(),
            thread.subject_id.as_str(),
            target_level,
            Some(source_digest),
            summary_text.as_str(),
            selected.first().map(|item| item.id.clone()),
            selected.last().map(|item| item.id.clone()),
            selected.len(),
            Some(serde_json::json!({
                "generator": "memory_engine_rollup_v1",
                "trigger_reason": trigger_reason,
                "source_level": level,
                "target_level": target_level,
            })),
        )
        .await?;

        let marked = match summaries::mark_summaries_rolled_up(
            db,
            tenant_id,
            source_id,
            thread.id.as_str(),
            selected_ids.as_slice(),
            created.id.as_str(),
        )
        .await
        {
            Ok(marked) => marked,
            Err(err) => {
                let _ =
                    summaries::delete_thread_summary(
                        db,
                        thread.id.as_str(),
                        created.id.as_str(),
                        Some(thread.tenant_id.as_str()),
                        Some(thread.source_id.as_str()),
                    )
                        .await;
                finish_rollup_job_run(
                    db,
                    job_run.id.as_str(),
                    FinishEngineJobRunRequest {
                        status: "failed".to_string(),
                        input_count: selected.len() as i64,
                        output_count: 0,
                        processed_count: selected.len() as i64,
                        success_count: 0,
                        error_count: selected.len() as i64,
                        metadata: Some(failed_metadata(
                            Some(selected.len()),
                            0,
                            selected.len() as i64,
                            0,
                            Some(trigger_reason),
                        )),
                        error_message: Some(format!("mark summaries rolled up failed: {}", err)),
                    },
                )
                .await;
                return Err(err);
            }
        };
        finish_rollup_job_run(
            db,
            job_run.id.as_str(),
            FinishEngineJobRunRequest {
                status: "done".to_string(),
                input_count: selected.len() as i64,
                output_count: 1,
                processed_count: selected.len() as i64,
                success_count: marked as i64,
                error_count: 0,
                metadata: Some(done_metadata(
                    selected.len(),
                    marked,
                    Some(created.id.as_str()),
                    trigger_reason,
                )),
                error_message: None,
            },
        )
        .await;
        if let Err(err) = crate::rollup_queue::publish_pending_rollup_for_summary(
            config,
            db,
            tenant_id,
            source_id,
            created.id.as_str(),
        )
        .await
        {
            tracing::warn!(
                summary_id = created.id.as_str(),
                error = err.as_str(),
                "Memory Engine left generated rollup event in Outbox for recovery"
            );
        }
        if let Err(err) = crate::subject_memory_queue::publish_pending_source_for_summary(
            config,
            db,
            tenant_id,
            source_id,
            created.id.as_str(),
        )
        .await
        {
            tracing::warn!(
                summary_id = created.id.as_str(),
                error = err.as_str(),
                "Memory Engine left rollup summary subject-memory event in Outbox for recovery"
            );
        }

        Ok(ThreadRollupResult {
            batches: 1,
            generated: 1,
            marked,
        })
    }
    .await;

    if let Err(err) = &result {
        if err == crate::services::memory_cloud_agent::MEMORY_CLOUD_AGENT_DEFERRED {
            return result;
        }
        finish_rollup_job_run(
            db,
            job_run.id.as_str(),
            FinishEngineJobRunRequest {
                status: "failed".to_string(),
                input_count: processed_count,
                output_count,
                processed_count,
                success_count: output_count,
                error_count: 1,
                metadata: Some(failed_metadata(
                    None,
                    0,
                    processed_count,
                    output_count,
                    None,
                )),
                error_message: Some(err.clone()),
            },
        )
        .await;
    }

    let release_result =
        threads::release_rollup_slot(db, tenant_id, source_id, thread_id, job_run.id.as_str())
            .await;
    match (result, release_result) {
        (Ok(value), Ok(())) => Ok(value),
        (Err(err), _) => Err(err),
        (Ok(_), Err(err)) => Err(format!("release thread rollup slot failed: {err}")),
    }
}
