// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

#[path = "../../../memory_engine/sdk/src/client/mod.rs"]
mod client;
#[path = "../../../memory_engine/sdk/src/models/mod.rs"]
mod models;

pub use self::client::MemoryEngineClient;
pub use self::models::{
    BatchSyncRecordsResponse, CompactTurnsResponse, ComposeContextBlock, ComposeContextMeta,
    ComposeContextPolicy, ComposeContextRequest, ComposeContextResponse,
    CountThreadRecordsResponse, DashboardOverviewResponse, DeleteThreadResponse, EngineJobPolicy,
    EngineJobRun, EngineModelProfile, EngineRecord, EngineSource, EngineSubjectMemory,
    EngineSubjectMemoryScope, EngineSummary, EngineThread, EngineThreadSnapshot,
    GenerateJobPolicyPromptRequest, GenerateJobPolicyPromptResponse, GetThreadResponse,
    JobRunsBundleResponse, ListJobRunsRequest, ListResponse, ListSourcesRequest,
    ListSummariesByThreadLabelRequest, QuerySubjectMemoriesRequest, RotateSourceSecretResponse,
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
    UpsertSubjectMemoryScopeRequest,
};
