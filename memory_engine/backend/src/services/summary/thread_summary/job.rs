// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::db::Db;
use crate::models::{CreateEngineJobRunRequest, EngineJobRun, FinishEngineJobRunRequest};
use crate::repositories::control_plane as cp_repo;
use serde::{Deserialize, Serialize};

use super::super::PendingRecordSelection;

pub(crate) const THREAD_DIRECT_TRIGGER: &str = "thread_direct";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(super) struct FrozenThreadSummarySelection {
    pub(super) selected_record_ids: Vec<String>,
    pub(super) oversized_record_ids: Vec<String>,
    pub(super) selected_token_count: i64,
    pub(super) oversized_token_count: i64,
}

impl FrozenThreadSummarySelection {
    pub(super) fn from_selection(selection: &PendingRecordSelection) -> Self {
        Self {
            selected_record_ids: selection
                .selected
                .iter()
                .map(|record| record.id.clone())
                .collect(),
            oversized_record_ids: selection
                .oversized
                .iter()
                .map(|record| record.id.clone())
                .collect(),
            selected_token_count: selection.selected_token_count,
            oversized_token_count: selection.oversized_token_count,
        }
    }

    pub(super) fn from_metadata(metadata: Option<&serde_json::Value>) -> Result<Self, String> {
        let frozen = metadata
            .and_then(|value| value.get("frozen_selection"))
            .cloned()
            .ok_or_else(|| "summary job has no frozen record selection".to_string())?;
        serde_json::from_value(frozen)
            .map_err(|error| format!("decode frozen summary selection failed: {error}"))
    }
}

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
    selection: &PendingRecordSelection,
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
                selection,
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
    selection: &PendingRecordSelection,
) -> serde_json::Value {
    serde_json::json!({
        "compat_job_type": "summary_l0",
        "compat_trigger_type": "manual_session",
        "pending_before_count": pending_before_count,
        "policy_token_limit": policy_token_limit,
        "policy_target_summary_tokens": policy_target_summary_tokens,
        "frozen_selection": FrozenThreadSummarySelection::from_selection(selection),
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

#[cfg(test)]
mod tests {
    use crate::models::EngineRecord;
    use crate::services::summary::PendingRecordSelection;

    use super::{start_metadata, FrozenThreadSummarySelection};

    fn record(id: &str) -> EngineRecord {
        EngineRecord {
            id: id.to_string(),
            thread_id: "thread".to_string(),
            tenant_id: "tenant".to_string(),
            source_id: "source".to_string(),
            external_record_id: None,
            role: "user".to_string(),
            record_type: "message".to_string(),
            content: id.to_string(),
            structured_payload: None,
            metadata: None,
            summary_status: "pending".to_string(),
            summary_id: None,
            summarized_at: None,
            created_at: "2026-01-01T00:00:00Z".to_string(),
        }
    }

    #[test]
    fn start_metadata_freezes_selected_and_oversized_record_ids() {
        let selection = PendingRecordSelection {
            selected: vec![record("selected-1"), record("selected-2")],
            oversized: vec![record("oversized-1")],
            selected_token_count: 321,
            oversized_token_count: 654,
        };
        let metadata = start_metadata(3, 6_000, 700, &selection);

        let frozen = FrozenThreadSummarySelection::from_metadata(Some(&metadata))
            .expect("frozen selection should decode");
        assert_eq!(frozen.selected_record_ids, ["selected-1", "selected-2"]);
        assert_eq!(frozen.oversized_record_ids, ["oversized-1"]);
        assert_eq!(frozen.selected_token_count, 321);
        assert_eq!(frozen.oversized_token_count, 654);
    }
}
