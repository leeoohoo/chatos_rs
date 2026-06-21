use crate::db::Db;
use crate::models::{
    CreateEngineJobRunRequest, FinishEngineJobRunRequest, RunSubjectMemoryJobRequest,
    RunSubjectMemoryJobResponse,
};
use crate::repositories::control_plane as cp_repo;

use super::{
    SubjectMemoryJobProgress, SUBJECT_MEMORY_COMPAT_JOB_TYPE, SUBJECT_MEMORY_JOB_TYPE,
    SUBJECT_MEMORY_MANUAL_TRIGGER, SUBJECT_MEMORY_SCOPE_TRIGGER,
};

pub(crate) async fn create_subject_memory_job_run(
    db: &Db,
    req: &RunSubjectMemoryJobRequest,
    relation_subject_id: &str,
    from_scope_runner: bool,
) -> Result<crate::models::EngineJobRun, String> {
    cp_repo::create_job_run(
        db,
        CreateEngineJobRunRequest {
            job_type: SUBJECT_MEMORY_JOB_TYPE.to_string(),
            trigger_type: if from_scope_runner {
                "scheduler".to_string()
            } else {
                "subject_direct".to_string()
            },
            tenant_id: Some(req.tenant_id.clone()),
            source_id: Some(req.source_id.clone()),
            thread_id: None,
            subject_id: Some(req.subject_id.clone()),
            thread_label: Some(req.source_thread_label.clone()),
            metadata: Some(base_metadata(
                from_scope_runner,
                req.memory_type.as_str(),
                relation_subject_id,
            )),
        },
    )
    .await
}

pub(crate) async fn finish_subject_memory_job_run(
    db: &Db,
    job_run_id: &str,
    req: FinishEngineJobRunRequest,
) {
    let _ = cp_repo::finish_job_run(db, job_run_id, req).await;
}

pub(crate) fn build_failed_job_run(
    req: &RunSubjectMemoryJobRequest,
    relation_subject_id: &str,
    from_scope_runner: bool,
    input_count: usize,
    progress: &SubjectMemoryJobProgress,
    selected_count: usize,
    error_message: String,
) -> FinishEngineJobRunRequest {
    FinishEngineJobRunRequest {
        status: "failed".to_string(),
        input_count: input_count as i64,
        output_count: progress.output_count(),
        processed_count: selected_count as i64,
        success_count: progress.output_count(),
        error_count: 1,
        metadata: Some(serde_json::json!({
            "compat_job_type": SUBJECT_MEMORY_COMPAT_JOB_TYPE,
            "compat_trigger_type": compat_trigger_type(from_scope_runner),
            "memory_type": req.memory_type,
            "relation_subject_id": relation_subject_id,
            "selected_count": selected_count,
            "marked_count": progress.marked_count(),
            "generated_level0": progress.generated_level0,
            "generated_rollups": progress.generated_rollups,
        })),
        error_message: Some(error_message),
    }
}

pub(crate) fn build_success_job_run(
    req: &RunSubjectMemoryJobRequest,
    relation_subject_id: &str,
    from_scope_runner: bool,
    input_count: usize,
    progress: &SubjectMemoryJobProgress,
    response: &RunSubjectMemoryJobResponse,
) -> FinishEngineJobRunRequest {
    FinishEngineJobRunRequest {
        status: "done".to_string(),
        input_count: input_count as i64,
        output_count: response.generated_memories as i64,
        processed_count: progress.processed_count as i64,
        success_count: response.generated_memories as i64,
        error_count: 0,
        metadata: Some(serde_json::json!({
            "compat_job_type": SUBJECT_MEMORY_COMPAT_JOB_TYPE,
            "compat_trigger_type": compat_trigger_type(from_scope_runner),
            "memory_type": req.memory_type,
            "relation_subject_id": relation_subject_id,
            "selected_count": progress.processed_count,
            "marked_count": response.marked_source_summaries + response.marked_source_memories,
            "generated_level0": response.generated_level0,
            "generated_rollups": response.generated_rollups,
        })),
        error_message: None,
    }
}

pub(crate) fn build_outer_failed_job_run(
    req: &RunSubjectMemoryJobRequest,
    relation_subject_id: &str,
    from_scope_runner: bool,
    input_count: usize,
    progress: &SubjectMemoryJobProgress,
    error_message: String,
) -> FinishEngineJobRunRequest {
    FinishEngineJobRunRequest {
        status: "failed".to_string(),
        input_count: input_count as i64,
        output_count: progress.output_count(),
        processed_count: progress.processed_count as i64,
        success_count: progress.output_count(),
        error_count: 1,
        metadata: Some(serde_json::json!({
            "compat_job_type": SUBJECT_MEMORY_COMPAT_JOB_TYPE,
            "compat_trigger_type": compat_trigger_type(from_scope_runner),
            "memory_type": req.memory_type,
            "relation_subject_id": relation_subject_id,
            "selected_count": progress.processed_count,
            "input_count": input_count,
            "marked_count": progress.marked_count(),
            "generated_level0": progress.generated_level0,
            "generated_rollups": progress.generated_rollups,
        })),
        error_message: Some(error_message),
    }
}

fn base_metadata(
    from_scope_runner: bool,
    memory_type: &str,
    relation_subject_id: &str,
) -> serde_json::Value {
    serde_json::json!({
        "compat_job_type": SUBJECT_MEMORY_COMPAT_JOB_TYPE,
        "compat_trigger_type": compat_trigger_type(from_scope_runner),
        "memory_type": memory_type,
        "relation_subject_id": relation_subject_id,
    })
}

fn compat_trigger_type(from_scope_runner: bool) -> &'static str {
    if from_scope_runner {
        SUBJECT_MEMORY_SCOPE_TRIGGER
    } else {
        SUBJECT_MEMORY_MANUAL_TRIGGER
    }
}
