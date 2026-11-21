// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team
//! Persistence interfaces owned by the Local Agent application layer.
use async_trait::async_trait;
use chatos_local_agent_protocol::{
    CancelConversationTurnCommand, CreateConversationCommand, CreateTaskGraphCommand,
    GuideConversationTurnCommand, LocalAgentArtifact, LocalAgentArtifactFile,
    LocalAgentArtifactPage, LocalAgentEventPayloadMode, LocalAgentEventRecord, LocalAgentRunClaim,
    LocalAgentRunListScope, LocalAgentRunPage, LocalAgentRunRecord, LocalAgentRunStatus,
    LocalAgentToolApprovalDecision, LocalAgentToolApprovalResult, LocalAgentToolClaim,
    LocalAgentToolCommitResult, LocalAgentToolInvocationRecord, LocalAgentToolOutcome,
    LocalConversationAttachmentRecord, LocalConversationDetail, LocalConversationHistoryPage,
    LocalConversationPage, LocalConversationRuntimeSettings, LocalConversationTurnStart,
    LocalConversationTurnUpdate, LocalMessageTaskGraphRunSummary, LocalNotepadImage,
    LocalNotepadNote, LocalNotepadNoteDetail, LocalPluginHookBundleRecord,
    LocalPluginHookBundleSpec, LocalPluginHookInvocationPage, LocalPluginInstallationPage,
    LocalPluginInstallationRecord, LocalPluginInstallationSpec, LocalRemoteConnection,
    LocalRequirementSurvey, LocalRequirementSurveyResolution, LocalRequirementSurveyStatus,
    LocalTaskGraph, LocalTaskGraphListScope, LocalTaskGraphPage, LocalTaskRecord, LocalTaskStatus,
    PutConversationRuntimeSettingsCommand, ResumeConversationTurnCommand,
    StartConversationTurnCommand, UpdateNotepadNoteCommand,
};
use serde_json::Value;
use thiserror::Error;
mod command;
mod memory_cache;
mod memory_outbox;
mod memory_transcript;
pub use chatos_local_agent_protocol::{
    LocalCapabilityPolicySnapshot, LocalJsonSchemaOutputFormat, LocalMemorySyncStatus,
    LocalModelConfigSnapshot, MAX_CAPABILITY_INSTRUCTIONS_BYTES, MAX_CAPABILITY_ITEMS,
    MAX_CONTROL_PLANE_SNAPSHOT_BYTES,
};
pub use command::{IdempotentCommand, LocalAgentArtifactWrite, RunTransition};
pub use memory_cache::{LocalMemoryContextCacheEntry, LocalMemoryContextCacheStore};
pub use memory_outbox::{
    LocalMemoryOutboxRecord, LocalMemoryOutboxStatus, LocalMemoryOutboxStore, LocalMemoryRunClaim,
};
pub use memory_transcript::{
    AuthoritativeConversationPage, AuthoritativeConversationRecord,
    LocalConversationTranscriptSource,
};

mod task_store_port;
pub use task_store_port::{LocalAgentTaskStore, LocalTaskListFilter};

mod store_ports;
pub use store_ports::*;
