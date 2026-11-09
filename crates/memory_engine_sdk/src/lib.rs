// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

#[cfg(feature = "client")]
mod client;
mod models;

#[cfg(feature = "client")]
pub use self::client::{MemoryEngineClient, RunPendingRollupsOptions};
pub use self::models::{
    managed_memory_policy_env_available, memory_feature_flag_config_key,
    memory_feature_flag_definition, memory_policy_config_key, memory_policy_env_key,
    BatchSyncRecordsResponse, CompactTurnsResponse, ComposeContextBlock, ComposeContextMeta,
    ComposeContextPolicy, ComposeContextRequest, ComposeContextResponse,
    CountThreadRecordsResponse, DashboardOverviewResponse, DeleteThreadResponse, EngineJobPolicy,
    EngineJobRun, EngineModelProfile, EngineRecord, EngineSource, EngineSubjectMemory,
    EngineSubjectMemoryScope, EngineSummary, EngineThread, EngineThreadSnapshot,
    GenerateJobPolicyPromptRequest, GenerateJobPolicyPromptResponse, GetThreadResponse,
    JobRunsBundleResponse, ListJobRunsRequest, ListResponse, ListSourcesRequest,
    ListSummariesByThreadLabelRequest, ManagedMemoryPolicy, ManagedMemoryPolicyBundle,
    MemoryFeatureFlagConfig, MemoryFeatureFlagDefinition, MemoryFeatureFlagScope, MemoryPolicyKind,
    QuerySubjectMemoriesRequest, RotateSourceSecretResponse, RunPendingRollupsResponse,
    RunPendingSummariesResponse, RunSubjectMemoryScopesResponse, RunThreadActiveSummaryResponse,
    RunThreadRepairSummaryResponse, RunThreadSummaryResponse, SdkAuthStatusResponse,
    SdkBatchSyncRecordsRequest, SdkComposeContextRequest, SdkCountThreadRecordsRequest,
    SdkDeleteThreadRecordsRequest, SdkDeleteThreadSummaryRequest,
    SdkGetLatestThreadSnapshotRequest, SdkGetRecordRequest, SdkGetThreadActiveSummaryStatusRequest,
    SdkGetThreadRequest, SdkGetThreadSnapshotByTurnRequest, SdkGetTurnProcessRecordsRequest,
    SdkListCompactTurnsRequest, SdkListSummariesByThreadLabelRequest, SdkListThreadRecordsRequest,
    SdkListThreadSummariesRequest, SdkListThreadsRequest, SdkQuerySubjectMemoriesRequest,
    SdkRunPendingRollupsRequest, SdkRunPendingSummariesRequest, SdkRunSubjectMemoryScopesRequest,
    SdkRunThreadActiveSummaryRequest, SdkRunThreadRepairSummaryRequest, SdkRunThreadSummaryRequest,
    SdkUpsertSubjectMemoryScopeRequest, SdkUpsertThreadRequest, SdkUpsertThreadSnapshotRequest,
    SystemListSummariesByThreadLabelRequest, SystemQuerySubjectMemoriesRequest,
    SystemUpsertSubjectMemoryScopeRequest, ThreadRecordsPageResponse, ThreadSnapshotLookupResponse,
    TurnProcessRecordsResponse, TurnRecordSlice, UpsertEngineJobPolicyRequest, UpsertRecordInput,
    UpsertSourceRequest, UpsertSubjectMemoryScopeRequest, UpsertThreadSnapshotRequest,
    MEMORY_COMPOSE_V2_INJECT_FLAG, MEMORY_COMPOSE_V2_SHADOW_FLAG, MEMORY_COVERAGE_V2_FLAG,
    MEMORY_FEATURE_FLAG_CONFIG_PREFIX, MEMORY_FEATURE_FLAG_DEFINITIONS,
    MEMORY_FEATURE_FLAG_SCHEMA_VERSION, MEMORY_GENERATION_COMMIT_V2_FLAG,
    MEMORY_LEXICAL_RETRIEVAL_V1_FLAG, MEMORY_PARTITION_DISPATCH_V2_FLAG,
    MEMORY_POLICY_CONFIG_PREFIX, MEMORY_PROJECT_ROLLUP_V2_FLAG, MEMORY_STRUCTURED_PAYLOAD_V1_FLAG,
};
