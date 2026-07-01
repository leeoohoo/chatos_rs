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

    let mut progress = SubjectMemoryJobProgress::new();
    let mut input_count = initial_prep.input_count;
    let relation_subject_id = initial_prep.settings.relation_subject_id.clone();

    let result: Result<RunSubjectMemoryJobResponse, String> = async {
        let mut prep = initial_prep;
        loop {
            input_count = input_count.max(prep.input_count);

            if let Some(selected) = prep.summary_selection.as_ref() {
                process_level0_selection(
                    config,
                    db,
                    &req,
                    &prep.settings,
                    selected.as_slice(),
                    from_scope_runner,
                    input_count,
                    job_run.id.as_str(),
                    &mut progress,
                )
                .await?;
                prep = prepare_subject_memory_job(db, &req).await?;
                continue;
            }

            if let Some(selection) = prep.rollup_selection.as_ref() {
                process_rollup_selection(
                    config,
                    db,
                    &req,
                    &prep.settings,
                    selection,
                    from_scope_runner,
                    input_count,
                    job_run.id.as_str(),
                    &mut progress,
                )
                .await?;
                prep = prepare_subject_memory_job(db, &req).await?;
                continue;
            }

            break;
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

    result
}
