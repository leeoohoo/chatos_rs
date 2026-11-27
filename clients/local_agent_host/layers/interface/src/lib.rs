// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

//! Stable, cross-platform contracts for the client-owned Local Agent Host.
//!
//! The protocol uses bounded, length-prefixed JSON message chunks. Small messages use a
//! single frame; larger messages are split into frames without allowing an unbounded
//! allocation. It deliberately contains no database, model-provider, plugin, or
//! platform-specific IPC implementation.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{fmt, str::FromStr};

mod account_scope;
mod artifact;
mod control_plane;
mod conversation;
mod hook;
mod memory;
mod notepad;
mod plugin;
mod remote_connection;
mod requirement_plan;
mod requirement_survey;
mod response;
mod run_query;
mod skill;
mod task;
mod task_acceptance;
mod task_followup;
mod task_write_scope;
mod tool;

pub use task_acceptance::{
    completed_task_acceptance_report, incomplete_task_acceptance_checks,
    task_acceptance_report_for_run, task_acceptance_report_is_blocked,
    task_acceptance_report_is_complete, task_acceptance_report_is_terminal,
    validate_task_acceptance_report, validate_task_final_response_limit,
};

pub use artifact::{
    CreateArtifactCommand, DeleteArtifactCommand, GetArtifactDataCommand, GetArtifactFileCommand,
    ListArtifactsCommand, LocalAgentArtifact, LocalAgentArtifactFile, LocalAgentArtifactPage,
    LOCAL_AGENT_ARTIFACT_MAX_BYTES, LOCAL_AGENT_ARTIFACT_MAX_PAGE_SIZE,
};
pub use control_plane::{
    GetCapabilityPolicySnapshotCommand, GetLatestCapabilityPolicySnapshotCommand,
    GetModelConfigSnapshotCommand, ListLatestModelConfigSnapshotsCommand,
    LocalCapabilityPolicySnapshot, LocalJsonSchemaOutputFormat, LocalModelConfigSnapshot,
    PutCapabilityPolicySnapshotCommand, PutModelConfigSnapshotCommand,
    MAX_CAPABILITY_INSTRUCTIONS_BYTES, MAX_CAPABILITY_ITEMS, MAX_CONTROL_PLANE_SNAPSHOT_BYTES,
};
pub use conversation::{
    CancelConversationTurnCommand, CreateConversationCommand, GetConversationAttachmentCommand,
    GetConversationCommand, GetConversationHistoryCommand, GetConversationRuntimeSettingsCommand,
    GuideConversationTurnCommand, ListConversationsCommand, LocalConversationAttachmentRecord,
    LocalConversationAttachmentSpec, LocalConversationDetail, LocalConversationHistoryPage,
    LocalConversationMessageRecord, LocalConversationMessageRole, LocalConversationPage,
    LocalConversationRecord, LocalConversationResourceBinding, LocalConversationResourceKind,
    LocalConversationRuntimeSettings, LocalConversationTurnRecord, LocalConversationTurnStart,
    LocalConversationTurnStatus, LocalConversationTurnUpdate,
    PutConversationRuntimeSettingsCommand, ResumeConversationTurnCommand,
    StartConversationTurnCommand, LOCAL_CONVERSATION_MAX_ATTACHMENTS,
    LOCAL_CONVERSATION_MAX_HISTORY_PAGE_SIZE,
};
pub use hook::{
    ListPluginHookBundleRecordsCommand, ListPluginHookBundlesCommand,
    ListPluginHookInvocationsCommand, LocalPluginHookBundleRecord, LocalPluginHookBundleSpec,
    LocalPluginHookDefinition, LocalPluginHookInvocationDiagnostic, LocalPluginHookInvocationPage,
    LocalPluginPlanningTaskTemplate, LocalPluginTaskPolicy, PutPluginHookBundleCommand,
    ResolveAgentTeamHookContextCommand, SetActivePluginHookBundlesCommand,
    LOCAL_PLUGIN_HOOK_HISTORY_MAX_PAGE_SIZE, LOCAL_PLUGIN_HOOK_MAX_BUNDLES,
    LOCAL_PLUGIN_HOOK_MAX_DEFINITIONS,
};
pub use memory::{GetMemorySyncStatusCommand, LocalMemorySyncStatus};
pub use notepad::{
    CreateNotepadFolderCommand, CreateNotepadNoteCommand, DeleteNotepadFolderCommand,
    DeleteNotepadNoteCommand, GetNotepadNoteCommand, InitializeNotepadCommand,
    ListNotepadFoldersCommand, ListNotepadNotesCommand, LocalNotepadImage, LocalNotepadNote,
    LocalNotepadNoteDetail, PutNotepadImageCommand, RenameNotepadFolderCommand,
    UpdateNotepadNoteCommand, LOCAL_NOTEPAD_MAX_CONTENT_BYTES, LOCAL_NOTEPAD_MAX_IMAGE_BYTES,
    LOCAL_NOTEPAD_MAX_LIST_LIMIT, LOCAL_NOTEPAD_MAX_TAGS,
};
pub use plugin::{
    GetPluginInstallationCommand, ListPluginInstallationsCommand, LocalPluginInstallationPage,
    LocalPluginInstallationRecord, LocalPluginInstallationSpec, LocalPluginInstallationSummary,
    PutPluginInstallationCommand, RemovePluginInstallationCommand,
};
pub use remote_connection::{
    CreateRemoteConnectionCommand, DeleteRemoteConnectionCommand, GetRemoteConnectionCommand,
    ListRemoteConnectionsCommand, LocalRemoteAuthenticationType, LocalRemoteConnection,
    LocalRemoteConnectionSpec, LocalRemoteHostKeyPolicy, UpdateRemoteConnectionCommand,
};
pub use requirement_plan::{
    ConfirmRequirementPlanCommand, GetRequirementPlanCommand, LocalRequirementPlan,
    SaveRequirementPlanCommand, LOCAL_REQUIREMENT_PLAN_MAX_BYTES,
    LOCAL_REQUIREMENT_PLAN_SAVE_TOOL_NAME,
};
pub use requirement_survey::{
    CreateRequirementSurveyCommand, GetRequirementSurveyCommand, ListRequirementSurveysCommand,
    LocalRequirementSurvey, LocalRequirementSurveyQuestion, LocalRequirementSurveyResolution,
    LocalRequirementSurveyResponseKind, LocalRequirementSurveyStatus,
    ResolveRequirementSurveyCommand, LOCAL_REQUIREMENT_SURVEY_CREATE_TOOL_NAME,
    LOCAL_REQUIREMENT_SURVEY_MAX_LIST_LIMIT, LOCAL_REQUIREMENT_SURVEY_MAX_QUESTIONS,
};
pub use response::{HostError, HostResponseEnvelope, HostResult, LocalAgentHostSchedulerSnapshot};
pub use run_query::{
    ListRunsCommand, LocalAgentRunListScope, LocalAgentRunPage, LocalAgentRunSummary,
};
pub use skill::{
    GetRunCapabilityDisclosuresCommand, GetRunSkillActivationsCommand,
    LocalDiscoveredToolSkillGate, LocalRunCapabilityDisclosure, LocalSkillArgumentSelector,
    LocalSkillBundleSnapshot, LocalSkillDocumentSnapshot, LocalSkillResourceSnapshot,
    LocalToolSkillActivationPolicy, LocalToolSkillBindingSnapshot,
    LOCAL_AGENT_SKILL_ACTIVATE_TOOL_NAME, LOCAL_AGENT_SKILL_READ_RESOURCE_TOOL_NAME,
    LOCAL_SKILL_BUNDLE_PROFILE_KEY, LOCAL_TASK_SKILL_BUNDLE_PROFILE_KEY, MAX_SKILL_BINDINGS,
    MAX_SKILL_BUNDLE_BYTES, MAX_SKILL_DOCUMENTS, MAX_SKILL_RESOURCES_PER_DOCUMENT,
};
pub use task_followup::{ResolveTaskFollowupCommand, TaskFollowupAction, TaskFollowupDecision};

pub use task::{
    resolve_local_task_graph_notification_policy, CancelTaskCommand, CreateTaskGraphCommand,
    GetMessageTaskGraphCommand, GetTaskGraphCommand, GetTaskRunsCommand, ListTaskGraphsCommand,
    LocalMessageTaskGraph, LocalMessageTaskGraphEdge, LocalMessageTaskGraphNode,
    LocalMessageTaskGraphRunSummary, LocalTaskDependency, LocalTaskGraph, LocalTaskGraphListScope,
    LocalTaskGraphNotificationPolicy, LocalTaskGraphPage, LocalTaskGraphStatus,
    LocalTaskGraphSummary, LocalTaskRecord, LocalTaskSpec, LocalTaskStatus, RestartTaskCommand,
    RetryTaskCommand,
};
pub use task_write_scope::{
    local_task_runtime_write_scope, local_task_write_scope, local_task_write_scope_overlap,
};
pub use tool::{
    ClaimNextToolCommand, CommitToolCommand, DecideToolApprovalCommand,
    ListPendingToolApprovalsCommand, LocalAgentToolApprovalDecision, LocalAgentToolApprovalResult,
    LocalAgentToolApprovalStatus, LocalAgentToolBatch, LocalAgentToolCall, LocalAgentToolClaim,
    LocalAgentToolCommitResult, LocalAgentToolExecutionRoute, LocalAgentToolInvocationRecord,
    LocalAgentToolOutcome, LocalAgentToolStatus, RenewToolClaimCommand,
};

pub const LOCAL_AGENT_PROTOCOL_VERSION: u32 = 53;
/// Maximum bytes in one physical IPC frame. The high bit of the frame prefix is reserved
/// as the continuation flag, so this limit must stay below `i32::MAX`.
pub const LOCAL_AGENT_MAX_FRAME_BYTES: usize = 4 * 1024 * 1024;
/// Maximum bytes in one reassembled JSON request or response.
///
/// This is intentionally separate from the physical frame limit: application payloads can
/// grow beyond one frame, while corrupt or hostile peers still cannot force an unbounded
/// allocation. Payloads above this boundary must use an artifact/reference workflow.
pub const LOCAL_AGENT_MAX_MESSAGE_BYTES: usize = 64 * 1024 * 1024;
pub const LOCAL_AGENT_MAX_INPUT_BYTES: usize = 256 * 1024;
pub const LOCAL_AGENT_MAX_EVENT_PAGE_SIZE: u32 = 500;

include!("core_protocol/envelope.rs");
include!("core_protocol/host_command.rs");
include!("core_protocol/scheduler.rs");
include!("core_protocol/run_commands.rs");
include!("core_protocol/step_outcome.rs");
include!("core_protocol/run_models.rs");

#[cfg(test)]
mod protocol_tests;
