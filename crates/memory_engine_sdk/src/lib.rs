// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

#[cfg(feature = "client")]
mod client;
mod models;
mod runtime_context;

#[cfg(feature = "client")]
pub use self::client::{MemoryEngineClient, RunPendingRollupsOptions};
pub use self::models::{
    memory_policy_config_key, BatchSyncRecordsResponse, CompactTurnsResponse, ComposeContextBlock,
    ComposeContextMeta, ComposeContextPolicy, ComposeContextRequest, ComposeContextResponse,
    ComposeTokenBudget, CountThreadRecordsResponse, DashboardOverviewResponse,
    DeleteThreadResponse, EngineJobPolicy, EngineJobRun, EngineModelProfile, EngineRecord,
    EngineSource, EngineSubjectMemory, EngineSubjectMemoryScope, EngineSummary, EngineThread,
    EngineThreadSnapshot, GetThreadResponse, JobRunsBundleResponse, ListJobRunsRequest,
    ListResponse, ListSourcesRequest, ListSummariesByThreadLabelRequest, ManagedMemoryPolicy,
    ManagedMemoryPolicyBundle, MemoryPolicyKind, QuerySubjectMemoriesRequest,
    RotateSourceSecretResponse, RunPendingRollupsResponse, RunPendingSummariesResponse,
    RunPendingSummaryRebuildResponse, RunSubjectMemoryScopesResponse,
    RunThreadActiveSummaryResponse, RunThreadSummaryResponse, SdkAuthStatusResponse,
    SdkBatchSyncRecordsRequest, SdkComposeContextRequest, SdkCountThreadRecordsRequest,
    SdkDeleteThreadRecordsRequest, SdkDeleteThreadSummaryRequest,
    SdkGetLatestThreadSnapshotRequest, SdkGetRecordRequest, SdkGetThreadActiveSummaryStatusRequest,
    SdkGetThreadRequest, SdkGetThreadSnapshotByTurnRequest, SdkGetTurnProcessRecordsRequest,
    SdkListCompactTurnsRequest, SdkListSummariesByThreadLabelRequest, SdkListThreadRecordsRequest,
    SdkListThreadSummariesRequest, SdkListThreadsRequest, SdkQuerySubjectMemoriesRequest,
    SdkRunPendingRollupsRequest, SdkRunPendingSummariesRequest, SdkRunPendingSummaryRebuildRequest,
    SdkRunSubjectMemoryScopesRequest, SdkRunThreadActiveSummaryRequest, SdkRunThreadSummaryRequest,
    SdkUpsertSubjectMemoryScopeRequest, SdkUpsertThreadRequest, SdkUpsertThreadSnapshotRequest,
    SystemListSummariesByThreadLabelRequest, SystemQuerySubjectMemoriesRequest,
    SystemUpsertSubjectMemoryScopeRequest, ThreadRecordsPageResponse, ThreadSnapshotLookupResponse,
    TurnProcessRecordsResponse, TurnRecordSlice, UpsertRecordInput, UpsertSourceRequest,
    UpsertSubjectMemoryScopeRequest, UpsertThreadSnapshotRequest, MEMORY_POLICY_CONFIG_PREFIX,
};
pub use self::runtime_context::{
    issue_runtime_context_assertion, runtime_context_assertion_key_id,
    verify_runtime_context_assertion, ExpectedRuntimeContext, IssueRuntimeContextAssertion,
    RuntimeContextAssertionClaims, RuntimeContextAssertionIssuer,
    MEMORY_ENGINE_RUNTIME_CONTEXT_AUDIENCE, RUNTIME_CONTEXT_MAX_TTL_SECONDS,
};
