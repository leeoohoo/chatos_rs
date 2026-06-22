use crate::db::Db;
use crate::models::{CreateEngineJobRunRequest, EngineJobRun, FinishEngineJobRunRequest};
use crate::repositories::control_plane as cp_repo;

pub(crate) const THREAD_DIRECT_TRIGGER: &str = "thread_direct";

pub(super) async fn create_thread_summary_job_run(
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    thread_id: &str,
    subject_id: &str,
    pending_before_count: i64,
    policy_token_limit: i64,
    policy_target_summary_tokens: i64,
    trigger_type: &str,
) -> Result<EngineJobRun, String> {
    cp_repo::create_job_run(
        db,
        CreateEngineJobRunRequest {
            job_type: "summary".to_string(),
            trigger_type: trigger_type.to_string(),
            tenant_id: Some(tenant_id.to_string()),
            source_id: Some(source_id.to_string()),
            thread_id: Some(thread_id.to_string()),
            subject_id: Some(subject_id.to_string()),
            thread_label: None,
            metadata: Some(start_metadata(
                pending_before_count,
                policy_token_limit,
                policy_target_summary_tokens,
            )),
        },
    )
    .await
}

pub(super) async fn finish_thread_summary_job_run(
    db: &Db,
    job_run_id: &str,
    req: FinishEngineJobRunRequest,
) {
    let _ = cp_repo::finish_job_run(db, job_run_id, req).await;
}

pub(super) fn start_metadata(
    pending_before_count: i64,
    policy_token_limit: i64,
    policy_target_summary_tokens: i64,
) -> serde_json::Value {
    serde_json::json!({
        "compat_job_type": "summary_l0",
        "compat_trigger_type": "manual_session",
        "pending_before_count": pending_before_count,
        "policy_token_limit": policy_token_limit,
        "policy_target_summary_tokens": policy_target_summary_tokens,
    })
}

pub(super) fn noop_metadata(
    pending_before_count: i64,
    pending_after_count: i64,
    skipped_count: usize,
) -> serde_json::Value {
    serde_json::json!({
        "compat_job_type": "summary_l0",
        "compat_trigger_type": "manual_session",
        "pending_before_count": pending_before_count,
        "selected_count": 0,
        "marked_count": skipped_count,
        "pending_after_count": pending_after_count,
        "skipped_oversized_count": skipped_count,
    })
}

pub(super) fn failed_metadata(
    pending_before_count: i64,
    selected_count: Option<usize>,
    selected_token_count: Option<i64>,
    skipped_count: usize,
    pending_after_count: Option<i64>,
    processed_count: i64,
    output_count: i64,
) -> serde_json::Value {
    let mut value = serde_json::json!({
        "compat_job_type": "summary_l0",
        "compat_trigger_type": "manual_session",
        "pending_before_count": pending_before_count,
        "processed_count": processed_count,
        "output_count": output_count,
    });
    if let Some(count) = selected_count {
        value["selected_count"] = serde_json::json!(count);
        value["marked_count"] = serde_json::json!(skipped_count);
        value["skipped_oversized_count"] = serde_json::json!(skipped_count);
    }
    if let Some(token_count) = selected_token_count {
        value["selected_token_count"] = serde_json::json!(token_count);
    }
    if let Some(count) = pending_after_count {
        value["pending_after_count"] = serde_json::json!(count);
    }
    value
}

pub(super) fn done_metadata(
    pending_before_count: i64,
    selected_count: usize,
    selected_token_count: i64,
    marked_count: usize,
    pending_after_count: i64,
    skipped_count: usize,
    summary_id: &str,
) -> serde_json::Value {
    serde_json::json!({
        "compat_job_type": "summary_l0",
        "compat_trigger_type": "manual_session",
        "pending_before_count": pending_before_count,
        "selected_count": selected_count,
        "selected_token_count": selected_token_count,
        "marked_count": marked_count,
        "pending_after_count": pending_after_count,
        "skipped_oversized_count": skipped_count,
        "generated_summary_id": summary_id,
    })
}
