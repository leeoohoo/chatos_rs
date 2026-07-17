// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

mod admin;
mod common;
mod context;
mod memory_policy;
mod records;
mod snapshots;
mod subject_memories;
mod summaries;
mod threads;

pub use self::admin::{
    DashboardOverviewResponse, EngineJobPolicy, EngineJobRun, EngineModelProfile, EngineSource,
    GenerateJobPolicyPromptRequest, GenerateJobPolicyPromptResponse, JobRunsBundleResponse,
    ListJobRunsRequest, ListSourcesRequest, RotateSourceSecretResponse, SdkAuthStatusResponse,
    UpsertEngineJobPolicyRequest, UpsertEngineModelProfileRequest, UpsertSourceRequest,
};
pub use self::common::{default_active, default_idle, default_pending, ListResponse};
pub use self::context::{
    ComposeContextBlock, ComposeContextMeta, ComposeContextPolicy, ComposeContextRequest,
    ComposeContextResponse, SdkComposeContextRequest,
};
pub use self::memory_policy::{
    managed_memory_policy_env_available, memory_policy_config_key, memory_policy_env_key,
    ManagedMemoryPolicy, ManagedMemoryPolicyBundle, MemoryPolicyKind, MEMORY_POLICY_CONFIG_PREFIX,
};
pub use self::records::{
    BatchSyncRecordsResponse, CompactTurnsResponse, CountThreadRecordsResponse, EngineRecord,
    SdkBatchSyncRecordsRequest, SdkCountThreadRecordsRequest, SdkDeleteThreadRecordsRequest,
    SdkGetRecordRequest, SdkGetTurnProcessRecordsRequest, SdkListCompactTurnsRequest,
    SdkListThreadRecordsRequest, ThreadRecordsPageResponse, TurnProcessRecordsResponse,
    TurnRecordSlice, UpsertRecordInput,
};
pub use self::snapshots::{
    EngineThreadSnapshot, SdkGetLatestThreadSnapshotRequest, SdkGetThreadSnapshotByTurnRequest,
    SdkUpsertThreadSnapshotRequest, ThreadSnapshotLookupResponse, UpsertThreadSnapshotRequest,
};
pub use self::subject_memories::{
    EngineSubjectMemory, EngineSubjectMemoryScope, QuerySubjectMemoriesRequest,
    RunSubjectMemoryScopesResponse, SdkQuerySubjectMemoriesRequest,
    SdkRunSubjectMemoryScopesRequest, SdkUpsertSubjectMemoryScopeRequest,
    SystemQuerySubjectMemoriesRequest, SystemUpsertSubjectMemoryScopeRequest,
    UpsertSubjectMemoryScopeRequest,
};
pub use self::summaries::{
    EngineSummary, ListSummariesByThreadLabelRequest, RunPendingRollupsResponse,
    RunPendingSummariesResponse, RunThreadActiveSummaryResponse, RunThreadRepairSummaryResponse,
    RunThreadSummaryResponse, SdkDeleteThreadSummaryRequest,
    SdkGetThreadActiveSummaryStatusRequest, SdkListSummariesByThreadLabelRequest,
    SdkListThreadSummariesRequest, SdkRunPendingRollupsRequest, SdkRunPendingSummariesRequest,
    SdkRunThreadActiveSummaryRequest, SdkRunThreadRepairSummaryRequest, SdkRunThreadSummaryRequest,
    SystemListSummariesByThreadLabelRequest,
};
pub use self::threads::{
    DeleteThreadResponse, EngineThread, GetThreadResponse, SdkGetThreadRequest,
    SdkListThreadsRequest, SdkUpsertThreadRequest,
};

#[cfg(test)]
mod contract_tests {
    use super::{ComposeContextPolicy, SdkComposeContextRequest, SdkUpsertThreadRequest};

    #[test]
    fn sdk_upsert_thread_request_serialization_snapshot_is_stable() {
        let request = SdkUpsertThreadRequest {
            tenant_id: "tenant-1".to_string(),
            subject_id: "subject-1".to_string(),
            thread_type: "chat".to_string(),
            external_thread_id: Some("external-1".to_string()),
            title: Some("Demo".to_string()),
            labels: Some(vec!["support".to_string()]),
            metadata: Some(serde_json::json!({"project_id": "project-1"})),
            status: Some("active".to_string()),
            created_at: None,
            updated_at: None,
            archived_at: None,
        };

        assert_eq!(
            serde_json::to_value(request).expect("thread request JSON"),
            serde_json::json!({
                "tenant_id": "tenant-1",
                "subject_id": "subject-1",
                "thread_type": "chat",
                "external_thread_id": "external-1",
                "title": "Demo",
                "labels": ["support"],
                "metadata": {"project_id": "project-1"},
                "status": "active",
                "created_at": null,
                "updated_at": null,
                "archived_at": null
            })
        );
    }

    #[test]
    fn compose_context_request_round_trips_without_field_drift() {
        let snapshot = serde_json::json!({
            "tenant_id": "tenant-1",
            "subject_id": "subject-1",
            "related_subject_ids": ["subject-2"],
            "thread_id": "thread-1",
            "policy": {
                "include_recent_records": true,
                "include_thread_summary": true,
                "include_subject_memory": false,
                "recent_record_limit": 20,
                "summary_limit": 3
            }
        });

        let request: SdkComposeContextRequest =
            serde_json::from_value(snapshot.clone()).expect("decode context request");
        let policy: &ComposeContextPolicy = request.policy.as_ref().expect("context policy");
        assert_eq!(policy.recent_record_limit, Some(20));
        assert_eq!(
            serde_json::to_value(request).expect("encode context request"),
            snapshot
        );
    }
}
