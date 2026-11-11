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
    JobRunsBundleResponse, ListJobRunsRequest, ListSourcesRequest, RotateSourceSecretResponse,
    SdkAuthStatusResponse, UpsertSourceRequest,
};
pub use self::common::{default_active, default_pending, ListResponse};
pub use self::context::{
    ComposeContextBlock, ComposeContextMeta, ComposeContextPolicy, ComposeContextRequest,
    ComposeContextResponse, ComposeTokenBudget, SdkComposeContextRequest,
};
pub use self::memory_policy::{
    memory_policy_config_key, ManagedMemoryPolicy, ManagedMemoryPolicyBundle, MemoryPolicyKind,
    MEMORY_POLICY_CONFIG_PREFIX,
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
    RunPendingSummariesResponse, RunPendingSummaryRebuildResponse, RunThreadActiveSummaryResponse,
    RunThreadSummaryResponse, SdkDeleteThreadSummaryRequest,
    SdkGetThreadActiveSummaryStatusRequest, SdkListSummariesByThreadLabelRequest,
    SdkListThreadSummariesRequest, SdkRunPendingRollupsRequest, SdkRunPendingSummariesRequest,
    SdkRunPendingSummaryRebuildRequest, SdkRunThreadActiveSummaryRequest,
    SdkRunThreadSummaryRequest, SystemListSummariesByThreadLabelRequest,
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
            "context_project_subject_id": "project-1",
            "thread_id": "thread-1",
            "query_text": "current goal",
            "current_turn_id": "turn-1",
            "model_capability_revision": "capability-v1",
            "runtime_context_assertion": "assertion",
            "policy": {
                "max_memory_tokens": 20000,
                "max_recent_record_tokens": 10000,
                "max_thread_summary_tokens": 5000,
                "max_project_memory_tokens": 3000,
                "max_agent_memory_tokens": 2000
            }
        });

        let request: SdkComposeContextRequest =
            serde_json::from_value(snapshot.clone()).expect("decode context request");
        let policy: &ComposeContextPolicy = &request.policy;
        assert_eq!(policy.max_recent_record_tokens, 10_000);
        assert_eq!(
            serde_json::to_value(request).expect("encode context request"),
            snapshot
        );
    }
}
