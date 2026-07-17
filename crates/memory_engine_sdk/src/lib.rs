// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

#[cfg(feature = "client")]
mod client;
mod models;

#[cfg(feature = "client")]
pub use self::client::{MemoryEngineClient, RunPendingRollupsOptions};
pub use self::models::{
    managed_memory_policy_env_available, memory_policy_config_key, memory_policy_env_key,
    BatchSyncRecordsResponse, CompactTurnsResponse, ComposeContextBlock, ComposeContextMeta,
    ComposeContextPolicy, ComposeContextRequest, ComposeContextResponse,
    CountThreadRecordsResponse, DashboardOverviewResponse, DeleteThreadResponse, EngineJobPolicy,
    EngineJobRun, EngineModelProfile, EngineRecord, EngineSource, EngineSubjectMemory,
    EngineSubjectMemoryScope, EngineSummary, EngineThread, EngineThreadSnapshot,
    GenerateJobPolicyPromptRequest, GenerateJobPolicyPromptResponse, GetThreadResponse,
    JobRunsBundleResponse, ListJobRunsRequest, ListResponse, ListSourcesRequest,
    ListSummariesByThreadLabelRequest, ManagedMemoryPolicy, ManagedMemoryPolicyBundle,
    MemoryPolicyKind, QuerySubjectMemoriesRequest, RotateSourceSecretResponse,
    RunPendingRollupsResponse, RunPendingSummariesResponse, RunSubjectMemoryScopesResponse,
    RunThreadActiveSummaryResponse, RunThreadRepairSummaryResponse, RunThreadSummaryResponse,
    SdkAuthStatusResponse, SdkBatchSyncRecordsRequest, SdkComposeContextRequest,
    SdkCountThreadRecordsRequest, SdkDeleteThreadRecordsRequest, SdkDeleteThreadSummaryRequest,
    SdkGetLatestThreadSnapshotRequest, SdkGetRecordRequest, SdkGetThreadActiveSummaryStatusRequest,
    SdkGetThreadRequest, SdkGetThreadSnapshotByTurnRequest, SdkGetTurnProcessRecordsRequest,
    SdkListCompactTurnsRequest, SdkListSummariesByThreadLabelRequest, SdkListThreadRecordsRequest,
    SdkListThreadSummariesRequest, SdkListThreadsRequest, SdkQuerySubjectMemoriesRequest,
    SdkRunPendingRollupsRequest, SdkRunPendingSummariesRequest, SdkRunSubjectMemoryScopesRequest,
    SdkRunThreadActiveSummaryRequest, SdkRunThreadRepairSummaryRequest, SdkRunThreadSummaryRequest,
    SdkUpsertSubjectMemoryScopeRequest, SdkUpsertThreadRequest, SdkUpsertThreadSnapshotRequest,
    SystemListSummariesByThreadLabelRequest, SystemQuerySubjectMemoriesRequest,
    SystemUpsertSubjectMemoryScopeRequest, ThreadRecordsPageResponse, ThreadSnapshotLookupResponse,
    TurnProcessRecordsResponse, TurnRecordSlice, UpsertEngineJobPolicyRequest,
    UpsertEngineModelProfileRequest, UpsertRecordInput, UpsertSourceRequest,
    UpsertSubjectMemoryScopeRequest, UpsertThreadSnapshotRequest, MEMORY_POLICY_CONFIG_PREFIX,
};
