// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::config::AppConfig;
use crate::db::Db;
use crate::models::{RunSubjectMemoryJobRequest, RunSubjectMemoryJobResponse};

use super::common::{
    build_outer_failed_job_run, build_success_job_run, create_subject_memory_job_run,
    empty_subject_memory_response, finish_subject_memory_job_run, log_noop,
    prepare_subject_memory_job, SubjectMemoryJobProgress,
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
    let scope_lock_owner = if from_scope_runner {
        let scope_key = req
            .scope_key
            .as_deref()
            .ok_or_else(|| "subject memory scope job has no scope_key".to_string())?;
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

        finish_subject_memory_job_run(
            db,
            job_run.id.as_str(),
            build_success_job_run(
                &req,
                relation_subject_id.as_str(),
                from_scope_runner,
                input_count,
                &progress,
                &response,
            ),
        )
        .await;

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

    result
}

pub(crate) async fn resume_subject_memory_job(
    config: &AppConfig,
    db: &Db,
    req: RunSubjectMemoryJobRequest,
    from_scope_runner: bool,
    job_run_id: &str,
    scope_lock_owner: Option<&str>,
) -> Result<RunSubjectMemoryJobResponse, String> {
    let initial_prep = prepare_subject_memory_job(db, &req).await?;
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
                job_run_id,
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
                job_run_id,
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
        finish_subject_memory_job_run(
            db,
            job_run_id,
            build_success_job_run(
                &req,
                relation_subject_id.as_str(),
                from_scope_runner,
                input_count,
                &progress,
                &response,
            ),
        )
        .await;
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
