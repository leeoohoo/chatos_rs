// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

mod common;
mod compose;
mod control_plane;
mod records;
mod sources;
mod subject_memories;
mod subject_memory_scopes;
mod subjects;
mod summaries;
mod thread_snapshots;
mod threads;

pub use self::common::{
    default_active, default_idle, default_pending, now_plus_seconds_rfc3339, now_rfc3339,
};
pub use self::compose::{
    ComposeContextBlock, ComposeContextMeta, ComposeContextPolicy, ComposeContextRequest,
    ComposeContextResponse,
};
pub use self::control_plane::{
    CreateEngineJobRunRequest, DashboardOverviewResponse, EngineJobPolicy, EngineJobRun,
    EngineModelProfile, FinishEngineJobRunRequest, GenerateJobPolicyPromptRequest,
    GenerateJobPolicyPromptResponse, JobRunsBundleResponse, UpsertEngineJobPolicyRequest,
    UpsertEngineModelProfileRequest, DEFAULT_ENGINE_MEMORY_ROLLUP_PROMPT_TEMPLATE,
    DEFAULT_ENGINE_MEMORY_ROLLUP_PROMPT_TEMPLATE_EN, DEFAULT_ENGINE_ROLLUP_PROMPT_TEMPLATE,
    DEFAULT_ENGINE_ROLLUP_PROMPT_TEMPLATE_EN, DEFAULT_ENGINE_SUBJECT_MEMORY_PROMPT_TEMPLATE,
    DEFAULT_ENGINE_SUBJECT_MEMORY_PROMPT_TEMPLATE_EN, DEFAULT_ENGINE_SUMMARY_PROMPT_TEMPLATE,
    DEFAULT_ENGINE_SUMMARY_PROMPT_TEMPLATE_EN, DEFAULT_ENGINE_THREAD_REPAIR_PROMPT_TEMPLATE,
    DEFAULT_ENGINE_THREAD_REPAIR_PROMPT_TEMPLATE_EN, PROMPT_LANGUAGE_EN, PROMPT_LANGUAGE_ZH,
};
pub use self::records::{
    BatchSyncRecordsRequest, BatchSyncRecordsResponse, CompactTurnsResponse, EngineCompactTurn,
    EngineRecord, ThreadRecordsPageResponse, TurnProcessRecordsResponse, TurnRecordSlice,
    UpsertRecordInput,
};
pub use self::sources::{EngineSource, RotateSourceSecretResponse, UpsertSourceRequest};
pub use self::subject_memories::{
    EngineSubjectMemory, MarkSubjectMemoriesRolledUpRequest, MarkSubjectMemoriesRolledUpResponse,
    QuerySubjectMemoriesRequest, RunSubjectMemoryJobRequest, RunSubjectMemoryJobResponse,
    UpsertSubjectMemoryRequest,
};
pub use self::subject_memory_scopes::{
    EngineSubjectMemoryScope, RunSubjectMemoryScopesRequest, RunSubjectMemoryScopesResponse,
    UpsertSubjectMemoryScopeRequest,
};
pub use self::subjects::{EngineSubject, UpsertSubjectRequest};
pub use self::summaries::{
    EngineSummary, GetThreadActiveSummaryStatusRequest, ListSummariesByThreadLabelRequest,
    MarkSummariesSubjectMemoryRequest, MarkSummariesSubjectMemoryResponse,
    RunPendingRollupsRequest, RunPendingRollupsResponse, RunPendingSummariesRequest,
    RunPendingSummariesResponse, RunThreadActiveSummaryRequest, RunThreadActiveSummaryResponse,
    RunThreadRepairSummaryRequest, RunThreadRepairSummaryResponse, RunThreadSummaryRequest,
    RunThreadSummaryResponse, UpsertThreadSummaryRequest,
};
pub use self::thread_snapshots::{
    EngineThreadSnapshot, ThreadSnapshotLookupResponse, UpsertThreadSnapshotRequest,
};
pub use self::threads::{
    DeleteThreadResponse, EngineThread, GetThreadResponse, ListThreadsByLabelRequest,
    UpsertThreadRequest,
};
