// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::config::AppConfig;
use crate::db::Db;
use crate::models::{RunSubjectMemoryJobRequest, RunSubjectMemoryJobResponse};
use crate::repositories::{generation_commits, generation_output_commits};

use super::super::builders::build_subject_memory_rollup;
use super::super::render::{digest_from_ids, subject_memory_to_rollup_block};
use super::super::{RollupSelection, SubjectMemoryJobSettings};
use super::common::{
    build_success_job_run, generated_subject_memory_request, SubjectMemoryJobProgress,
};
use super::level0::{generation_profile, insert_generation_profile, project_partition_key};

pub(crate) const SUBJECT_MEMORY_ROLLUP_GENERATION_STEP: &str = "subject_memory_rollup_final";
pub(crate) use crate::services::ai_pipeline::profiles::SUBJECT_MEMORY_ROLLUP_PIPELINE_REVISION;
const GENERATION_FINALIZE_LEASE_SECONDS: i64 = 300;

#[allow(clippy::too_many_arguments)]
pub(crate) async fn process_rollup_selection(
    config: &AppConfig,
    db: &Db,
    req: &RunSubjectMemoryJobRequest,
    settings: &SubjectMemoryJobSettings,
    selection: &RollupSelection,
    from_scope_runner: bool,
    input_count: usize,
    job_run_id: &str,
    scope_lock_owner: Option<&str>,
    progress: &mut SubjectMemoryJobProgress,
) -> Result<(), String> {
    let level = selection.level;
    let target_level = level + 1;
    let selected_ids = selection
        .selected
        .iter()
        .map(|item| item.id.clone())
        .collect::<Vec<_>>();
    let source_digest = digest_from_ids(
        format!("{}:rollup:l{}->{}", req.memory_type, level, target_level).as_str(),
        selected_ids.as_slice(),
    )
    .ok_or_else(|| "build subject memory rollup digest failed".to_string())?;
    let profile = generation_profile(req.memory_type.as_str())?;
    let partition = project_partition_key(req, settings);
    let scope_kind = if req.memory_type == "project_summary" {
        "project"
    } else {
        "agent"
    };
    let descriptor = generation_descriptor(
        selection,
        settings,
        profile,
        partition.as_deref(),
        target_level,
    )?;
    let items = selection
        .selected
        .iter()
        .map(subject_memory_to_rollup_block)
        .collect::<Vec<_>>();
    let build = build_subject_memory_rollup(
        config,
        db,
        req.tenant_id.as_str(),
        settings.prompt_title.as_str(),
        req.memory_type.as_str(),
        items.as_slice(),
        settings.token_limit,
        level,
        target_level,
        job_run_id,
        serde_json::json!({
            "resume_kind": "subject_memory_job",
            "job_run_id": job_run_id,
            "tenant_id": req.tenant_id,
            "source_id": req.source_id,
            "request": req,
            "from_scope_runner": from_scope_runner,
            "scope_lock_owner": scope_lock_owner,
            "scope_key": req.scope_key,
            "payload_scope": {
                "kind": scope_kind,
                "subject_id": req.subject_id,
                "project_partition_key": partition,
            },
            "generation": descriptor,
        }),
    )
    .await?;
    let structured_payload = build.structured_payload;
    let generated_text = crate::models::structured_payload::render_payload(&structured_payload)?;
    let memory_key = format!("{}:l{}:{}", req.memory_type, target_level, source_digest);
    let mut memory_req = generated_subject_memory_request(
        req,
        settings,
        generated_text.clone(),
        target_level,
        source_digest.as_str(),
    );
    insert_generation_profile(&mut memory_req.metadata, profile);
    let generation = claim_generation(
        db,
        req.tenant_id.as_str(),
        req.source_id.as_str(),
        job_run_id,
    )
    .await?;
    memory_req.text = crate::models::structured_payload::render_payload(&structured_payload)?;

    progress.add_processed(selection.selected.len());
    progress.generated_rollups = 1;
    progress.marked_source_memories = selection.selected.len();
    let response = RunSubjectMemoryJobResponse {
        subject_id: req.subject_id.clone(),
        generated_level0: 0,
        generated_rollups: 1,
        generated_memories: 1,
        marked_source_summaries: 0,
        marked_source_memories: selection.selected.len(),
    };
    generation_output_commits::commit_subject_memory_generation_final(
        db,
        &generation,
        generation_output_commits::SubjectMemoryGenerationOutput {
            subject_id: req.subject_id.as_str(),
            memory_key: memory_key.as_str(),
            request: memory_req,
            source_digest: Some(source_digest),
            rollup_status: "pending",
            generation_profile: profile,
            project_partition_key: partition.as_deref(),
            payload_schema: crate::models::structured_payload::MEMORY_PAYLOAD_SCHEMA,
            structured_payload: &structured_payload,
            pipeline_revision: SUBJECT_MEMORY_ROLLUP_PIPELINE_REVISION,
        },
        generation_output_commits::SubjectMemoryGenerationFinalization {
            sources: generation_output_commits::SubjectMemoryGenerationSources::SubjectMemories {
                memory_ids: selected_ids.as_slice(),
            },
            scope_claim: match (req.scope_key.as_deref(), scope_lock_owner) {
                (Some(scope_key), Some(lock_owner)) => {
                    Some(generation_output_commits::SubjectMemoryScopeClaim {
                        scope_key,
                        lock_owner,
                    })
                }
                (None, None) => None,
                _ => {
                    return Err(
                        "subject memory rollup scope claim identity is incomplete".to_string()
                    )
                }
            },
            job_result: build_success_job_run(
                req,
                settings.relation_subject_id.as_str(),
                from_scope_runner,
                input_count,
                progress,
                &response,
            ),
        },
    )
    .await?;
    Ok(())
}

fn generation_descriptor(
    selection: &RollupSelection,
    settings: &SubjectMemoryJobSettings,
    generation_profile: &str,
    project_partition_key: Option<&str>,
    target_level: i64,
) -> Result<serde_json::Value, String> {
    let inputs = selection
        .selected
        .iter()
        .map(|memory| {
            serde_json::json!({
                "id": memory.id,
                "memory_key": memory.memory_key,
                "memory_type": memory.memory_type,
                "level": memory.level,
                "text": memory.text,
                "structured_payload": memory.structured_payload,
                "source_digest": memory.source_digest,
                "metadata": memory.metadata,
                "created_at": memory.created_at,
                "updated_at": memory.updated_at,
            })
        })
        .collect::<Vec<_>>();
    let (manifest, digest) = crate::services::generation_identity::input_manifest(inputs)?;
    Ok(crate::services::generation_identity::descriptor(
        SUBJECT_MEMORY_ROLLUP_GENERATION_STEP,
        "subject_memory_rollup",
        generation_profile,
        SUBJECT_MEMORY_ROLLUP_PIPELINE_REVISION,
        project_partition_key,
        manifest,
        digest,
        serde_json::json!({
            "max_model_input_tokens": settings.token_limit,
            "source_level": selection.level,
            "target_level": target_level,
        }),
    ))
}

async fn claim_generation(
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
        SUBJECT_MEMORY_ROLLUP_GENERATION_STEP,
    )
    .await?
    .ok_or_else(|| {
        "subject memory rollup generation was not frozen before model execution".to_string()
    })?;
    if generation.status == "committed" {
        return Ok(generation);
    }
    generation_commits::claim_generation(
        db,
        tenant_id,
        source_id,
        generation.generation_id.as_str(),
        format!("subject-memory-rollup-finalizer:{job_run_id}").as_str(),
        GENERATION_FINALIZE_LEASE_SECONDS,
    )
    .await?
    .ok_or_else(|| "subject memory rollup finalization lease is occupied".to_string())
}
