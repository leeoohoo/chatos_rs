// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::models::{FinishEngineJobRunRequest, RunThreadRepairSummaryResponse};

use super::super::common::{
    THREAD_REPAIR_COMPAT_JOB_TYPE, THREAD_REPAIR_COMPAT_TRIGGER_TYPE,
    THREAD_REPAIR_SELECTION_POLICY,
};

pub(super) async fn finalize_job_run(
    db: &crate::db::Db,
    job_run_id: &str,
    req: FinishEngineJobRunRequest,
) {
    let _ = crate::repositories::control_plane::finish_job_run(db, job_run_id, req).await;
}

pub(super) fn noop_response(
    thread_id: &str,
    job_run_id: Option<&str>,
) -> RunThreadRepairSummaryResponse {
    RunThreadRepairSummaryResponse {
        thread_id: thread_id.to_string(),
        accepted: false,
        running: false,
        job_run_id: job_run_id.map(ToOwned::to_owned),
        generated: false,
        summary_id: None,
        source_record_count: 0,
    }
}

pub(super) fn running_response(
    thread_id: &str,
    job_run_id: String,
    source_record_count: usize,
) -> RunThreadRepairSummaryResponse {
    RunThreadRepairSummaryResponse {
        thread_id: thread_id.to_string(),
        accepted: true,
        running: true,
        job_run_id: Some(job_run_id),
        generated: false,
        summary_id: None,
        source_record_count,
    }
}

pub(super) fn success_response(
    thread_id: &str,
    job_run_id: &str,
    summary_id: String,
    source_record_count: usize,
) -> RunThreadRepairSummaryResponse {
    RunThreadRepairSummaryResponse {
        thread_id: thread_id.to_string(),
        accepted: true,
        running: false,
        job_run_id: Some(job_run_id.to_string()),
        generated: true,
        summary_id: Some(summary_id),
        source_record_count,
    }
}

pub(super) fn start_metadata(pending_before_count: i64) -> serde_json::Value {
    serde_json::json!({
        "compat_job_type": THREAD_REPAIR_COMPAT_JOB_TYPE,
        "compat_trigger_type": THREAD_REPAIR_COMPAT_TRIGGER_TYPE,
        "pending_before_count": pending_before_count,
        "selection_policy": THREAD_REPAIR_SELECTION_POLICY,
    })
}

pub(super) fn noop_metadata(pending_before_count: i64) -> serde_json::Value {
    serde_json::json!({
        "compat_job_type": THREAD_REPAIR_COMPAT_JOB_TYPE,
        "compat_trigger_type": THREAD_REPAIR_COMPAT_TRIGGER_TYPE,
        "pending_before_count": pending_before_count,
        "selected_count": 0,
        "selection_policy": THREAD_REPAIR_SELECTION_POLICY,
    })
}

pub(super) fn success_metadata(
    pending_before_count: i64,
    selected_count: usize,
    selected_token_count: i64,
    marked_count: usize,
    pending_after_count: i64,
    summary_id: &str,
) -> serde_json::Value {
    serde_json::json!({
        "compat_job_type": THREAD_REPAIR_COMPAT_JOB_TYPE,
        "compat_trigger_type": THREAD_REPAIR_COMPAT_TRIGGER_TYPE,
        "pending_before_count": pending_before_count,
        "selected_count": selected_count,
        "selected_token_count": selected_token_count,
        "marked_count": marked_count,
        "pending_after_count": pending_after_count,
        "selection_policy": THREAD_REPAIR_SELECTION_POLICY,
        "generated_summary_id": summary_id,
    })
}

pub(super) fn failed_metadata(
    pending_before_count: i64,
    processed_count: i64,
    output_count: i64,
) -> serde_json::Value {
    serde_json::json!({
        "compat_job_type": THREAD_REPAIR_COMPAT_JOB_TYPE,
        "compat_trigger_type": THREAD_REPAIR_COMPAT_TRIGGER_TYPE,
        "pending_before_count": pending_before_count,
        "processed_count": processed_count,
        "output_count": output_count,
    })
}
