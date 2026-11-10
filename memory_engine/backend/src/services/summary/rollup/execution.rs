// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::config::AppConfig;
use crate::db::Db;
use crate::models::FinishEngineJobRunRequest;
use crate::repositories::{
    generation_commits, generation_output_commits, thread_partitions, threads,
};

use super::super::builders::{
    build_rollup_summary_text, THREAD_ROLLUP_GENERATION_PROFILE, THREAD_ROLLUP_PIPELINE_REVISION,
};
use super::super::render::{build_summary_digest, summary_to_rollup_block};
use super::super::selectors::select_rollup_batch;
use super::super::{
    PreparedThreadRollup, RollupSettings, ThreadRollupDrainError, ThreadRollupResult,
};
use super::job::{create_rollup_job_run, done_metadata, failed_metadata, finish_rollup_job_run};

const THREAD_ROLLUP_GENERATION_STEP: &str = "thread_rollup_final";
const GENERATION_FINALIZE_LEASE_SECONDS: i64 = 300;

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
        settings.source_batch_token_limit.max(500),
        settings.count_limit.max(0),
        settings.keep_level0_count.max(0),
        settings.max_level.max(1),
    )
    .await?;
    let Some((level, project_partition_key, selected, trigger_reason)) = selection else {
        return Ok(None);
    };

    Ok(Some(PreparedThreadRollup {
        thread_id: thread_id.to_string(),
        project_partition_key,
        level,
        selected,
        trigger_reason: trigger_reason.to_string(),
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
        project_partition_key,
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
    }
    let resumed = cloud_job_run_id.is_some();
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
    if job_run.status == "done" {
        return Ok(ThreadRollupResult {
            batches: 1,
            generated: 1,
            marked: selected.len(),
        });
    }
    if job_run.status != "running" {
        return Ok(empty_rollup_result());
    }

    let owns_claim = if resumed {
        thread_partitions::refresh_rollup_claim(
            db,
            tenant_id,
            source_id,
            thread_id,
            project_partition_key.as_str(),
            job_run.id.as_str(),
            config.rollup_lock_timeout_secs,
        )
        .await?
    } else {
        thread_partitions::try_claim_rollup(
            db,
            tenant_id,
            source_id,
            thread_id,
            project_partition_key.as_str(),
            job_run.id.as_str(),
            config.rollup_lock_timeout_secs,
        )
        .await?
        .is_some()
    };
    if !owns_claim {
        finish_rollup_job_run(
            db,
            job_run.id.as_str(),
            FinishEngineJobRunRequest {
                status: "failed".to_string(),
                input_count: selected.len() as i64,
                output_count: 0,
                processed_count: 0,
                success_count: 0,
                error_count: 1,
                metadata: Some(failed_metadata(
                    Some(selected.len()),
                    0,
                    0,
                    0,
                    Some("partition_claim_occupied"),
                )),
                error_message: Some("thread rollup partition claim is occupied".to_string()),
            },
        )
        .await;
        if resumed {
            return Ok(empty_rollup_result());
        }
        return Err("thread rollup partition claim is occupied".to_string());
    }

    let selected_ids = selected
        .iter()
        .map(|summary| summary.id.clone())
        .collect::<Vec<_>>();
    let target_level = level + 1;
    let source_digest = build_summary_digest(
        thread.id.as_str(),
        level,
        target_level,
        selected_ids.as_slice(),
    );
    let items = selected
        .iter()
        .map(summary_to_rollup_block)
        .collect::<Vec<_>>();
    let descriptor = thread_rollup_generation_descriptor(
        selected.as_slice(),
        settings.token_limit,
        level,
        target_level,
        trigger_reason.as_str(),
        project_partition_key.as_str(),
    )?;
    let mut cloud_settings = settings.clone();
    cloud_settings.cloud_owner_entity_id = Some(job_run.id.clone());
    cloud_settings.cloud_source_id = Some(source_id.to_string());
    cloud_settings.cloud_thread_id = Some(thread_id.to_string());
    let summary_build = match build_rollup_summary_text(
        config,
        db,
        tenant_id,
        thread.title.as_deref(),
        thread.subject_id.as_str(),
        items.as_slice(),
        &cloud_settings,
        level,
        target_level,
        descriptor,
    )
    .await
    {
        Ok(build) => build,
        Err(error) if error == crate::services::memory_cloud_agent::MEMORY_CLOUD_AGENT_DEFERRED => {
            return Err(error);
        }
        Err(error) => {
            fail_rollup_job(
                db,
                tenant_id,
                source_id,
                thread_id,
                project_partition_key.as_str(),
                job_run.id.as_str(),
                selected.len(),
                trigger_reason.as_str(),
                error.clone(),
            )
            .await;
            return Err(error);
        }
    };

    let generation = claim_rollup_generation(db, tenant_id, source_id, job_run.id.as_str()).await?;
    let structured_payload = summary_build.structured_payload;
    let summary_text = crate::models::structured_payload::render_payload(&structured_payload)?;
    let job_result = FinishEngineJobRunRequest {
        status: "done".to_string(),
        input_count: selected.len() as i64,
        output_count: 1,
        processed_count: selected.len() as i64,
        success_count: selected.len() as i64,
        error_count: 0,
        metadata: Some(done_metadata(
            selected.len(),
            selected.len(),
            None,
            trigger_reason.as_str(),
        )),
        error_message: None,
    };
    let created = generation_output_commits::commit_thread_rollup_generation_final(
        db,
        &generation,
        generation_output_commits::ThreadSummaryGenerationOutput {
            thread_id,
            subject_id: thread.subject_id.as_str(),
            summary_type: "thread_incremental",
            level: target_level,
            source_digest: Some(source_digest),
            summary_text: summary_text.as_str(),
            source_record_start_id: selected_ids.first().cloned(),
            source_record_end_id: selected_ids.last().cloned(),
            source_record_count: selected.len(),
            metadata: Some(serde_json::json!({
                "trigger_reason": trigger_reason,
                "source_level": level,
                "target_level": target_level,
                "generation_profile": THREAD_ROLLUP_GENERATION_PROFILE,
            })),
            generation_reason: "rollup",
            generation_profile: THREAD_ROLLUP_GENERATION_PROFILE,
            project_partition_key: project_partition_key.as_str(),
            payload_schema: crate::models::structured_payload::MEMORY_PAYLOAD_SCHEMA,
            structured_payload: &structured_payload,
            pipeline_revision: THREAD_ROLLUP_PIPELINE_REVISION,
        },
        generation_output_commits::ThreadRollupGenerationFinalization {
            summary_ids: selected_ids.as_slice(),
            job_result,
        },
    )
    .await?;

    if let Err(error) = crate::rollup_queue::publish_pending_rollup_for_summary(
        config,
        db,
        tenant_id,
        source_id,
        created.id.as_str(),
    )
    .await
    {
        tracing::warn!(
            summary_id = created.id,
            error,
            "rollup Outbox publish deferred"
        );
    }
    if let Err(error) = crate::subject_memory_queue::publish_pending_source_for_summary(
        config,
        db,
        tenant_id,
        source_id,
        created.id.as_str(),
    )
    .await
    {
        tracing::warn!(
            summary_id = created.id,
            error,
            "rollup subject-memory Outbox publish deferred"
        );
    }

    Ok(ThreadRollupResult {
        batches: 1,
        generated: 1,
        marked: selected.len(),
    })
}

fn thread_rollup_generation_descriptor(
    summaries: &[crate::models::EngineSummary],
    token_limit: i64,
    source_level: i64,
    target_level: i64,
    trigger_reason: &str,
    project_partition_key: &str,
) -> Result<serde_json::Value, String> {
    let inputs = summaries
        .iter()
        .map(|summary| {
            serde_json::json!({
                "id": summary.id,
                "level": summary.level,
                "summary_type": summary.summary_type,
                "summary_text": summary.summary_text,
                "structured_payload": summary.structured_payload,
                "source_digest": summary.source_digest,
                "created_at": summary.created_at,
                "updated_at": summary.updated_at,
            })
        })
        .collect::<Vec<_>>();
    let (manifest, digest) = crate::services::generation_identity::input_manifest(inputs)?;
    Ok(crate::services::generation_identity::descriptor(
        THREAD_ROLLUP_GENERATION_STEP,
        "thread_rollup",
        THREAD_ROLLUP_GENERATION_PROFILE,
        THREAD_ROLLUP_PIPELINE_REVISION,
        Some(project_partition_key),
        manifest,
        digest,
        serde_json::json!({
            "max_model_input_tokens": token_limit,
            "source_level": source_level,
            "target_level": target_level,
            "trigger_reason": trigger_reason,
        }),
    ))
}

async fn claim_rollup_generation(
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    job_run_id: &str,
) -> Result<generation_commits::GenerationCommit, String> {
    let generation = generation_commits::get_generation_by_job_step(
        db,
        tenant_id,
        source_id,
        job_run_id,
        THREAD_ROLLUP_GENERATION_STEP,
    )
    .await?
    .ok_or_else(|| "thread rollup generation was not frozen before model execution".to_string())?;
    if generation.status == "committed" {
        return Ok(generation);
    }
    generation_commits::claim_generation(
        db,
        tenant_id,
        source_id,
        generation.generation_id.as_str(),
        format!("thread-rollup-finalizer:{job_run_id}").as_str(),
        GENERATION_FINALIZE_LEASE_SECONDS,
    )
    .await?
    .ok_or_else(|| "thread rollup generation finalization lease is occupied".to_string())
}

#[allow(clippy::too_many_arguments)]
async fn fail_rollup_job(
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    thread_id: &str,
    project_partition_key: &str,
    job_run_id: &str,
    selected_count: usize,
    trigger_reason: &str,
    error: String,
) {
    finish_rollup_job_run(
        db,
        job_run_id,
        FinishEngineJobRunRequest {
            status: "failed".to_string(),
            input_count: selected_count as i64,
            output_count: 0,
            processed_count: 0,
            success_count: 0,
            error_count: selected_count as i64,
            metadata: Some(failed_metadata(
                Some(selected_count),
                0,
                0,
                0,
                Some(trigger_reason),
            )),
            error_message: Some(error),
        },
    )
    .await;
    let _ = thread_partitions::release_rollup_claim(
        db,
        tenant_id,
        source_id,
        thread_id,
        project_partition_key,
        job_run_id,
    )
    .await;
}
