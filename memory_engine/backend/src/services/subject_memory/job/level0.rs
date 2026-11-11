// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::config::AppConfig;
use crate::db::Db;
use crate::models::{RunSubjectMemoryJobRequest, RunSubjectMemoryJobResponse};
use crate::repositories::{generation_commits, generation_output_commits};

use super::super::builders::build_subject_memory_from_summaries;
use super::super::render::{digest_from_ids, summary_to_subject_memory_block};
use super::super::{PendingSourceSummary, SubjectMemoryJobSettings};
use super::common::{
    build_success_job_run, generated_subject_memory_request, SubjectMemoryJobProgress,
};

pub(crate) const SUBJECT_MEMORY_L0_GENERATION_STEP: &str = "subject_memory_l0_final";
pub(crate) use crate::services::ai_pipeline::profiles::SUBJECT_MEMORY_L0_PIPELINE_REVISION;
const GENERATION_FINALIZE_LEASE_SECONDS: i64 = 300;

#[allow(clippy::too_many_arguments)]
pub(crate) async fn process_level0_selection(
    config: &AppConfig,
    db: &Db,
    req: &RunSubjectMemoryJobRequest,
    settings: &SubjectMemoryJobSettings,
    selected: &[PendingSourceSummary],
    from_scope_runner: bool,
    input_count: usize,
    job_run_id: &str,
    scope_lock_owner: Option<&str>,
    progress: &mut SubjectMemoryJobProgress,
) -> Result<(), String> {
    let selected_ids = selected
        .iter()
        .map(|item| item.id.clone())
        .collect::<Vec<_>>();
    let source_digest = digest_from_ids(
        format!("{}:l0", req.memory_type).as_str(),
        selected_ids.as_slice(),
    )
    .ok_or_else(|| "build subject memory l0 digest failed".to_string())?;
    let generation_profile = generation_profile(req.memory_type.as_str())?;
    let generation_kind = generation_kind(req.memory_type.as_str())?;
    let project_partition_key = project_partition_key(req, settings);
    let scope_kind = if req.memory_type == "project_summary" {
        "project"
    } else {
        "agent"
    };
    let generation = generation_descriptor(
        selected,
        settings,
        generation_kind,
        generation_profile,
        project_partition_key.as_deref(),
    )?;
    let selected_texts = selected
        .iter()
        .map(summary_to_subject_memory_block)
        .collect::<Vec<_>>();
    let build = build_subject_memory_from_summaries(
        config,
        db,
        req.tenant_id.as_str(),
        settings.prompt_title.as_str(),
        req.memory_type.as_str(),
        selected_texts.as_slice(),
        settings.token_limit,
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
                "project_partition_key": project_partition_key,
            },
            "generation": generation,
        }),
    )
    .await?;
    let structured_payload = build.structured_payload;
    let generated_text = crate::models::structured_payload::render_payload(&structured_payload)?;
    let memory_key = format!("{}:l0:{}", req.memory_type, source_digest);
    let mut memory_req = generated_subject_memory_request(
        req,
        settings,
        generated_text.clone(),
        0,
        source_digest.as_str(),
    );
    insert_generation_profile(&mut memory_req.metadata, generation_profile);

    let claimed_generation = claim_generation(
        db,
        req.tenant_id.as_str(),
        req.source_id.as_str(),
        job_run_id,
    )
    .await?;
    memory_req.text = crate::models::structured_payload::render_payload(&structured_payload)?;
    progress.add_processed(selected.len());
    progress.generated_level0 = 1;
    progress.marked_source_summaries = selected.len();
    let response = RunSubjectMemoryJobResponse {
        subject_id: req.subject_id.clone(),
        generated_level0: 1,
        generated_rollups: 0,
        generated_memories: 1,
        marked_source_summaries: selected.len(),
        marked_source_memories: 0,
    };
    let scope_claim = match (req.scope_key.as_deref(), scope_lock_owner) {
        (Some(scope_key), Some(lock_owner)) => {
            Some(generation_output_commits::SubjectMemoryScopeClaim {
                scope_key,
                lock_owner,
            })
        }
        (None, None) => None,
        _ => return Err("subject memory scope claim identity is incomplete".to_string()),
    };
    generation_output_commits::commit_subject_memory_generation_final(
        db,
        &claimed_generation,
        generation_output_commits::SubjectMemoryGenerationOutput {
            subject_id: req.subject_id.as_str(),
            memory_key: memory_key.as_str(),
            request: memory_req,
            source_digest: Some(source_digest),
            rollup_status: "pending",
            generation_profile,
            project_partition_key: project_partition_key.as_deref(),
            payload_schema: crate::models::structured_payload::MEMORY_PAYLOAD_SCHEMA,
            structured_payload: &structured_payload,
            pipeline_revision: SUBJECT_MEMORY_L0_PIPELINE_REVISION,
        },
        generation_output_commits::SubjectMemoryGenerationFinalization {
            sources: generation_output_commits::SubjectMemoryGenerationSources::Summaries {
                summary_ids: selected_ids.as_slice(),
                scope_key: req.scope_key.as_deref(),
            },
            scope_claim,
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
    selected: &[PendingSourceSummary],
    settings: &SubjectMemoryJobSettings,
    generation_kind: &str,
    generation_profile: &str,
    project_partition_key: Option<&str>,
) -> Result<serde_json::Value, String> {
    let inputs = selected
        .iter()
        .map(|summary| {
            serde_json::json!({
                "id": summary.id,
                "tenant_id": summary.tenant_id,
                "source_id": summary.source_id,
                "thread_id": summary.thread_id,
                "summary_type": summary.summary_type,
                "level": summary.level,
                "summary_text": summary.summary_text,
                "structured_payload": summary.structured_payload,
                "created_at": summary.created_at,
                "metadata": summary.metadata,
            })
        })
        .collect::<Vec<_>>();
    let (manifest, digest) = crate::services::generation_identity::input_manifest(inputs)?;
    Ok(crate::services::generation_identity::descriptor(
        SUBJECT_MEMORY_L0_GENERATION_STEP,
        generation_kind,
        generation_profile,
        SUBJECT_MEMORY_L0_PIPELINE_REVISION,
        project_partition_key,
        manifest,
        digest,
        serde_json::json!({
            "max_model_input_tokens": settings.token_limit,
            "count_limit": settings.count_limit,
            "source_summary_type": settings.source_summary_type,
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
        SUBJECT_MEMORY_L0_GENERATION_STEP,
    )
    .await?
    .ok_or_else(|| "subject memory generation was not frozen before model execution".to_string())?;
    if generation.status == "committed" {
        return Ok(generation);
    }
    generation_commits::claim_generation(
        db,
        tenant_id,
        source_id,
        generation.generation_id.as_str(),
        format!("subject-memory-finalizer:{job_run_id}").as_str(),
        GENERATION_FINALIZE_LEASE_SECONDS,
    )
    .await?
    .ok_or_else(|| "subject memory generation finalization lease is occupied".to_string())
}

pub(crate) fn generation_profile(memory_type: &str) -> Result<&'static str, String> {
    match memory_type {
        "agent_memory" => {
            Ok(crate::repositories::subject_memory_scopes::AGENT_STABLE_MEMORY_PROFILE)
        }
        "project_summary" => Ok("project_summary_v2"),
        _ => Err("unsupported level-0 subject memory type".to_string()),
    }
}

fn generation_kind(memory_type: &str) -> Result<&'static str, String> {
    match memory_type {
        "agent_memory" => Ok("agent_memory"),
        "project_summary" => Ok("project_memory"),
        _ => Err("unsupported level-0 subject memory type".to_string()),
    }
}

pub(crate) fn project_partition_key(
    req: &RunSubjectMemoryJobRequest,
    settings: &SubjectMemoryJobSettings,
) -> Option<String> {
    (req.memory_type == "project_summary").then(|| {
        if settings.relation_subject_id.starts_with("project:") {
            settings.relation_subject_id.clone()
        } else {
            format!("project:{}", settings.relation_subject_id)
        }
    })
}

pub(crate) fn insert_generation_profile(
    metadata: &mut Option<serde_json::Value>,
    generation_profile: &str,
) {
    let value = metadata.get_or_insert_with(|| serde_json::json!({}));
    if let Some(object) = value.as_object_mut() {
        object.insert(
            "generation_profile".to_string(),
            serde_json::Value::String(generation_profile.to_string()),
        );
    }
}
