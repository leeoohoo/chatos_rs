// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::db::Db;
use crate::models::{
    CreateEngineJobRunRequest, FinishEngineJobRunRequest, RunSubjectMemoryJobRequest,
    RunSubjectMemoryJobResponse,
};
use crate::repositories::control_plane as cp_repo;

use super::{SubjectMemoryJobProgress, SUBJECT_MEMORY_JOB_TYPE};

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
                "queue".to_string()
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
            "scope_runner": from_scope_runner,
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
            "scope_runner": from_scope_runner,
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

pub(crate) fn build_scope_busy_job_run(
    req: &RunSubjectMemoryJobRequest,
    relation_subject_id: &str,
    input_count: usize,
) -> FinishEngineJobRunRequest {
    FinishEngineJobRunRequest {
        status: "done".to_string(),
        input_count: input_count as i64,
        output_count: 0,
        processed_count: 0,
        success_count: 0,
        error_count: 0,
        metadata: Some(serde_json::json!({
            "scope_runner": true,
            "memory_type": req.memory_type,
            "relation_subject_id": relation_subject_id,
            "deferred_reason": "scope_slot_occupied",
        })),
        error_message: None,
    }
}

fn base_metadata(
    from_scope_runner: bool,
    memory_type: &str,
    relation_subject_id: &str,
) -> serde_json::Value {
    serde_json::json!({
        "pipeline": "subject_memory",
        "scope_runner": from_scope_runner,
        "memory_type": memory_type,
        "relation_subject_id": relation_subject_id,
    })
}

#[cfg(test)]
mod tests {
    use super::build_scope_busy_job_run;
    use crate::models::RunSubjectMemoryJobRequest;

    #[test]
    fn occupied_scope_is_deferred_without_recording_a_failure() {
        let req = RunSubjectMemoryJobRequest {
            tenant_id: "tenant".into(),
            source_id: "source".into(),
            subject_id: "project:1".into(),
            memory_type: "project_summary".into(),
            source_thread_label: "project:1".into(),
            relation_subject_id: None,
            source_summary_type: None,
            prompt_title: None,
            memory_metadata: None,
            scope_key: Some("identity:project:1".into()),
        };

        let result = build_scope_busy_job_run(&req, "project_memory:project:1", 264);

        assert_eq!(result.status, "done");
        assert_eq!(result.error_count, 0);
        assert_eq!(result.error_message, None);
        assert_eq!(
            result.metadata.unwrap()["deferred_reason"],
            "scope_slot_occupied"
        );
    }
}
