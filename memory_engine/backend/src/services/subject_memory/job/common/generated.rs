// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::db::Db;
use crate::models::{now_rfc3339, RunSubjectMemoryJobRequest, UpsertSubjectMemoryRequest};
use crate::services::ai_pipeline::SummaryBuildResult;

use super::{
    build_failed_job_run, finish_subject_memory_job_run, tombstone_generated_subject_memory,
    SubjectMemoryJobProgress,
};
use crate::services::subject_memory::SubjectMemoryJobSettings;

pub(crate) fn generated_subject_memory_request(
    req: &RunSubjectMemoryJobRequest,
    settings: &SubjectMemoryJobSettings,
    text: String,
    level: i64,
    source_digest: &str,
) -> UpsertSubjectMemoryRequest {
    UpsertSubjectMemoryRequest {
        id: None,
        tenant_id: req.tenant_id.clone(),
        source_id: req.source_id.clone(),
        memory_type: req.memory_type.clone(),
        text,
        level: Some(level),
        source_digest: Some(source_digest.to_string()),
        confidence: None,
        last_seen_at: Some(now_rfc3339()),
        metadata: super::super::super::render::build_memory_metadata(
            settings.memory_metadata.clone(),
            settings.relation_subject_id.as_str(),
            req.source_thread_label.as_str(),
        ),
        rollup_status: Some("pending".to_string()),
        rollup_memory_key: None,
        rolled_up_at: None,
        status: Some("active".to_string()),
        created_at: None,
        updated_at: None,
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn record_subject_memory_selection_failure(
    db: &Db,
    req: &RunSubjectMemoryJobRequest,
    relation_subject_id: &str,
    from_scope_runner: bool,
    input_count: usize,
    progress: &SubjectMemoryJobProgress,
    selected_count: usize,
    job_run_id: &str,
    error_message: String,
) {
    finish_subject_memory_job_run(
        db,
        job_run_id,
        build_failed_job_run(
            req,
            relation_subject_id,
            from_scope_runner,
            input_count,
            progress,
            selected_count,
            error_message,
        ),
    )
    .await;
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn finish_subject_memory_build(
    db: &Db,
    req: &RunSubjectMemoryJobRequest,
    settings: &SubjectMemoryJobSettings,
    from_scope_runner: bool,
    input_count: usize,
    progress: &SubjectMemoryJobProgress,
    selected_count: usize,
    job_run_id: &str,
    result: Result<SummaryBuildResult, String>,
) -> Result<SummaryBuildResult, String> {
    match result {
        Ok(build) => Ok(build),
        Err(error) => {
            record_subject_memory_selection_failure(
                db,
                req,
                settings.relation_subject_id.as_str(),
                from_scope_runner,
                input_count,
                progress,
                selected_count,
                job_run_id,
                error.clone(),
            )
            .await;
            Err(error)
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn rollback_generated_subject_memory_after_mark_failure(
    db: &Db,
    req: &RunSubjectMemoryJobRequest,
    settings: &SubjectMemoryJobSettings,
    from_scope_runner: bool,
    input_count: usize,
    progress: &SubjectMemoryJobProgress,
    selected_count: usize,
    job_run_id: &str,
    memory_key: &str,
    source_digest: &str,
    level: i64,
    error_message: String,
) {
    tombstone_generated_subject_memory(db, req, memory_key, source_digest, level).await;
    record_subject_memory_selection_failure(
        db,
        req,
        settings.relation_subject_id.as_str(),
        from_scope_runner,
        input_count,
        progress,
        selected_count,
        job_run_id,
        error_message,
    )
    .await;
}
