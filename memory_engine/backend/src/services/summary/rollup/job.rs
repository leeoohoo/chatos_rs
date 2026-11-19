// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::db::Db;
use crate::models::{CreateEngineJobRunRequest, FinishEngineJobRunRequest};
use crate::repositories::control_plane as cp_repo;

pub(crate) const SCHEDULER_TRIGGER: &str = "scheduler";

pub(super) async fn create_rollup_job_run(
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    thread_id: &str,
    subject_id: &str,
    trigger_type: &str,
) -> Result<crate::models::EngineJobRun, String> {
    cp_repo::create_job_run(
        db,
        CreateEngineJobRunRequest {
            job_type: "rollup".to_string(),
            trigger_type: trigger_type.to_string(),
            tenant_id: Some(tenant_id.to_string()),
            source_id: Some(source_id.to_string()),
            thread_id: Some(thread_id.to_string()),
            subject_id: Some(subject_id.to_string()),
            thread_label: None,
            metadata: Some(serde_json::json!({"pipeline": "thread_rollup"})),
        },
    )
    .await
}

pub(super) async fn finish_rollup_job_run(
    db: &Db,
    job_run_id: &str,
    req: FinishEngineJobRunRequest,
) {
    let _ = cp_repo::finish_job_run(db, job_run_id, req).await;
}

pub(super) fn done_metadata(
    selected_count: usize,
    marked_count: usize,
    rollup_summary_id: Option<&str>,
    trigger_reason: &str,
) -> serde_json::Value {
    let mut value = serde_json::json!({
        "selected_count": selected_count,
        "marked_count": marked_count,
        "pending_after_count": 0,
        "trigger_reason": trigger_reason,
    });
    if let Some(summary_id) = rollup_summary_id {
        value["rollup_summary_id"] = serde_json::json!(summary_id);
    }
    value
}

pub(super) fn partition_claim_deferred_job(
    selected_count: usize,
) -> FinishEngineJobRunRequest {
    FinishEngineJobRunRequest {
        status: "done".to_string(),
        input_count: selected_count as i64,
        output_count: 0,
        processed_count: 0,
        success_count: 0,
        error_count: 0,
        metadata: Some(done_metadata(
            selected_count,
            0,
            None,
            "partition_claim_occupied",
        )),
        error_message: None,
    }
}

pub(super) fn failed_metadata(
    selected_count: Option<usize>,
    marked_count: usize,
    processed_count: i64,
    output_count: i64,
    trigger_reason: Option<&str>,
) -> serde_json::Value {
    let mut value = serde_json::json!({
        "marked_count": marked_count,
        "pending_after_count": 0,
        "processed_count": processed_count,
        "output_count": output_count,
    });
    if let Some(count) = selected_count {
        value["selected_count"] = serde_json::json!(count);
    }
    if let Some(reason) = trigger_reason {
        value["trigger_reason"] = serde_json::json!(reason);
    }
    value
}

#[cfg(test)]
mod tests {
    use super::partition_claim_deferred_job;

    #[test]
    fn occupied_partition_is_deferred_without_recording_a_failure() {
        let result = partition_claim_deferred_job(137);

        assert_eq!(result.status, "done");
        assert_eq!(result.error_count, 0);
        assert_eq!(result.error_message, None);
        assert_eq!(
            result.metadata.unwrap()["trigger_reason"],
            "partition_claim_occupied"
        );
    }
}
