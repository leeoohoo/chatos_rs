// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::config::AppConfig;
use crate::db::Db;
use crate::models::{RunSubjectMemoryJobRequest, RunSubjectMemoryJobResponse};

use super::common::{
    build_outer_failed_job_run, create_subject_memory_job_run, empty_subject_memory_response,
    finish_subject_memory_job_run, log_noop, prepare_subject_memory_job, SubjectMemoryJobProgress,
};
use super::level0::process_level0_selection;
use super::rollup::process_rollup_selection;

pub async fn run_subject_memory_job(
    config: &AppConfig,
    db: &Db,
    req: RunSubjectMemoryJobRequest,
) -> Result<RunSubjectMemoryJobResponse, String> {
    run_subject_memory_job_internal(config, db, req, false).await
}

pub(crate) async fn run_subject_memory_job_internal(
    config: &AppConfig,
    db: &Db,
    req: RunSubjectMemoryJobRequest,
    from_scope_runner: bool,
) -> Result<RunSubjectMemoryJobResponse, String> {
    let initial_prep = prepare_subject_memory_job(db, &req).await?;
    if initial_prep.summary_selection.is_none() && initial_prep.rollup_selection.is_none() {
        log_noop(&req);
        return Ok(empty_subject_memory_response(req.subject_id.clone()));
    }

    let job_run = create_subject_memory_job_run(
        db,
        &req,
        initial_prep.settings.relation_subject_id.as_str(),
        from_scope_runner,
    )
    .await?;
    let scope_lock_owner = if let Some(scope_key) = req.scope_key.as_deref() {
        let acquired =
            crate::repositories::subject_memory_scopes::try_acquire_subject_memory_scope_slot(
                db,
                req.tenant_id.as_str(),
                req.source_id.as_str(),
                scope_key,
                job_run.id.as_str(),
                crate::services::memory_cloud_agent::cloud_subject_scope_lock_timeout_secs(config),
            )
            .await?;
        if !acquired {
            finish_subject_memory_job_run(
                db,
                job_run.id.as_str(),
                build_outer_failed_job_run(
                    &req,
                    initial_prep.settings.relation_subject_id.as_str(),
                    true,
                    initial_prep.input_count,
                    &SubjectMemoryJobProgress::new(),
                    "subject memory scope slot already occupied".to_string(),
                ),
            )
            .await;
            return Err("subject memory scope slot already occupied".to_string());
        }
        Some(job_run.id.as_str())
    } else {
        None
    };

    let mut progress = SubjectMemoryJobProgress::new();
    let input_count = initial_prep.input_count;
    let relation_subject_id = initial_prep.settings.relation_subject_id.clone();

    let result: Result<RunSubjectMemoryJobResponse, String> = async {
        if let Some(selected) = initial_prep.summary_selection.as_ref() {
            process_level0_selection(
                config,
                db,
                &req,
                &initial_prep.settings,
                selected.as_slice(),
                from_scope_runner,
                input_count,
                job_run.id.as_str(),
                scope_lock_owner,
                &mut progress,
            )
            .await?;
        } else if let Some(selection) = initial_prep.rollup_selection.as_ref() {
            process_rollup_selection(
                config,
                db,
                &req,
                &initial_prep.settings,
                selection,
                from_scope_runner,
                input_count,
                job_run.id.as_str(),
                scope_lock_owner,
                &mut progress,
            )
            .await?;
        }

        let response = RunSubjectMemoryJobResponse {
            subject_id: req.subject_id.clone(),
            generated_level0: progress.generated_level0,
            generated_rollups: progress.generated_rollups,
            generated_memories: progress.generated_level0 + progress.generated_rollups,
            marked_source_summaries: progress.marked_source_summaries,
            marked_source_memories: progress.marked_source_memories,
        };

        Ok(response)
    }
    .await;

    if let Err(err) = &result {
        if err == crate::services::memory_cloud_agent::MEMORY_CLOUD_AGENT_DEFERRED {
            return result;
        }
        finish_subject_memory_job_run(
            db,
            job_run.id.as_str(),
            build_outer_failed_job_run(
                &req,
                relation_subject_id.as_str(),
                from_scope_runner,
                input_count,
                &progress,
                err.clone(),
            ),
        )
        .await;
    }

    if result.is_err() {
        if let (Some(lock_owner), Some(scope_key)) = (scope_lock_owner, req.scope_key.as_deref()) {
            crate::repositories::subject_memory_scopes::release_subject_memory_scope_slot(
                db,
                req.tenant_id.as_str(),
                req.source_id.as_str(),
                scope_key,
                lock_owner,
            )
            .await?;
        }
    }

    result
}

pub(crate) async fn resume_subject_memory_job(
    config: &AppConfig,
    db: &Db,
    req: RunSubjectMemoryJobRequest,
    from_scope_runner: bool,
    job_run_id: &str,
    generation_id: &str,
    scope_lock_owner: Option<&str>,
) -> Result<RunSubjectMemoryJobResponse, String> {
    let job_run = crate::repositories::control_plane::get_job_run_by_id(db, job_run_id)
        .await?
        .ok_or_else(|| format!("subject memory job run not found: {job_run_id}"))?;
    if job_run.job_type != "subject_memory"
        || job_run.tenant_id.as_deref() != Some(req.tenant_id.as_str())
        || job_run.source_id.as_deref() != Some(req.source_id.as_str())
        || job_run.subject_id.as_deref() != Some(req.subject_id.as_str())
    {
        return Err("subject memory job scope does not match its Cloud Agent callback".to_string());
    }
    if job_run.status != "running" {
        return Ok(empty_subject_memory_response(req.subject_id.clone()));
    }

    let generation = crate::repositories::generation_commits::get_generation(
        db,
        req.tenant_id.as_str(),
        req.source_id.as_str(),
        generation_id,
    )
    .await?
    .ok_or_else(|| "subject memory frozen generation is missing".to_string())?;
    if generation.job_run_id != job_run_id {
        return Err("subject memory generation does not belong to callback job".to_string());
    }
    let source_ids = generation
        .input_manifest
        .get("inputs")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| "subject memory generation input manifest is missing".to_string())?
        .iter()
        .map(|input| {
            input
                .get("id")
                .and_then(serde_json::Value::as_str)
                .map(ToOwned::to_owned)
                .ok_or_else(|| "subject memory frozen source id is invalid".to_string())
        })
        .collect::<Result<Vec<_>, _>>()?;
    let settings = super::super::settings::build_settings_with_policy(db, &req).await?;
    let mut progress = SubjectMemoryJobProgress::new();
    let input_count = source_ids.len();
    let relation_subject_id = settings.relation_subject_id.clone();
    let effective_scope_lock_owner = req.scope_key.as_deref().map(|_| job_run_id);
    if let Some(scope_key) = req.scope_key.as_deref() {
        let callback_owner_matches = scope_lock_owner
            .map(|owner| owner == job_run_id)
            .unwrap_or(true);
        let owns_scope = if callback_owner_matches {
            crate::repositories::subject_memory_scopes::refresh_subject_memory_scope_slot(
                db,
                req.tenant_id.as_str(),
                req.source_id.as_str(),
                scope_key,
                job_run_id,
                crate::services::memory_cloud_agent::cloud_subject_scope_lock_timeout_secs(config),
            )
            .await?
        } else {
            false
        };
        if !owns_scope {
            finish_subject_memory_job_run(
                db,
                job_run_id,
                build_outer_failed_job_run(
                    &req,
                    relation_subject_id.as_str(),
                    from_scope_runner,
                    input_count,
                    &progress,
                    "subject memory scope was superseded before terminal callback".to_string(),
                ),
            )
            .await;
            return Ok(empty_subject_memory_response(req.subject_id.clone()));
        }
    }
    let result: Result<RunSubjectMemoryJobResponse, String> = async {
        if matches!(
            generation.generation_kind.as_str(),
            "project_memory" | "agent_memory"
        ) {
            let selected = crate::repositories::summaries::list_summaries_by_ids_in_source(
                db,
                req.tenant_id.as_str(),
                req.source_id.as_str(),
                source_ids.as_slice(),
            )
            .await?
            .into_iter()
            .map(|item| super::super::PendingSourceSummary {
                id: item.id,
                tenant_id: item.tenant_id,
                source_id: item.source_id,
                thread_id: item.thread_id,
                summary_type: item.summary_type,
                level: item.level,
                summary_text: item.summary_text,
                structured_payload: item.structured_payload,
                created_at: item.created_at,
                metadata: item.metadata,
            })
            .collect::<Vec<_>>();
            process_level0_selection(
                config,
                db,
                &req,
                &settings,
                selected.as_slice(),
                from_scope_runner,
                input_count,
                job_run_id,
                effective_scope_lock_owner,
                &mut progress,
            )
            .await?;
        } else if generation.generation_kind == "subject_memory_rollup" {
            let selected = crate::repositories::subject_memories::list_subject_memories_by_ids(
                db,
                req.tenant_id.as_str(),
                req.source_id.as_str(),
                req.subject_id.as_str(),
                source_ids.as_slice(),
            )
            .await?;
            let level = selected
                .first()
                .map(|memory| memory.level)
                .ok_or_else(|| "subject memory rollup frozen sources are empty".to_string())?;
            if selected.iter().any(|memory| memory.level != level) {
                return Err("subject memory rollup frozen sources mix levels".to_string());
            }
            let selection = super::super::RollupSelection { level, selected };
            process_rollup_selection(
                config,
                db,
                &req,
                &settings,
                &selection,
                from_scope_runner,
                input_count,
                job_run_id,
                effective_scope_lock_owner,
                &mut progress,
            )
            .await?;
        } else {
            return Err("subject memory callback generation kind is invalid".to_string());
        }
        let response = RunSubjectMemoryJobResponse {
            subject_id: req.subject_id.clone(),
            generated_level0: progress.generated_level0,
            generated_rollups: progress.generated_rollups,
            generated_memories: progress.generated_level0 + progress.generated_rollups,
            marked_source_summaries: progress.marked_source_summaries,
            marked_source_memories: progress.marked_source_memories,
        };
        Ok(response)
    }
    .await;
    if let Err(error) = &result {
        if error != crate::services::memory_cloud_agent::MEMORY_CLOUD_AGENT_DEFERRED {
            finish_subject_memory_job_run(
                db,
                job_run_id,
                build_outer_failed_job_run(
                    &req,
                    relation_subject_id.as_str(),
                    from_scope_runner,
                    input_count,
                    &progress,
                    error.clone(),
                ),
            )
            .await;
        }
    }
    result
}

pub(crate) async fn subject_memory_job_has_work(
    db: &Db,
    req: &RunSubjectMemoryJobRequest,
) -> Result<bool, String> {
    let prep = prepare_subject_memory_job(db, req).await?;
    Ok(prep.summary_selection.is_some() || prep.rollup_selection.is_some())
}
