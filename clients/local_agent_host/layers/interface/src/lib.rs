// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

//! Stable, cross-platform contracts for the client-owned Local Agent Host.
//!
//! The protocol uses length-prefixed JSON frames. It deliberately contains no
//! database, model-provider, plugin, or platform-specific IPC implementation.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{fmt, str::FromStr};

mod account_scope;
mod artifact;
mod control_plane;
mod conversation;
mod memory;
mod notepad;
mod plugin;
mod remote_connection;
mod requirement_survey;
mod response;
mod run_query;
mod task;
mod tool;

pub use artifact::{
    CreateArtifactCommand, DeleteArtifactCommand, GetArtifactDataCommand, ListArtifactsCommand,
    LocalAgentArtifact, LocalAgentArtifactPage, LOCAL_AGENT_ARTIFACT_MAX_BYTES,
    LOCAL_AGENT_ARTIFACT_MAX_PAGE_SIZE,
};
pub use control_plane::{
    GetCapabilityPolicySnapshotCommand, GetModelConfigSnapshotCommand,
    LocalCapabilityPolicySnapshot, LocalJsonSchemaOutputFormat, LocalModelConfigSnapshot,
    PutCapabilityPolicySnapshotCommand, PutModelConfigSnapshotCommand,
    MAX_CAPABILITY_INSTRUCTIONS_BYTES, MAX_CAPABILITY_ITEMS, MAX_CONTROL_PLANE_SNAPSHOT_BYTES,
};
pub use conversation::{
    CancelConversationTurnCommand, CreateConversationCommand, GetConversationCommand,
    GetConversationHistoryCommand, GetConversationRuntimeSettingsCommand,
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
pub use requirement_survey::{
    CreateRequirementSurveyCommand, GetRequirementSurveyCommand, ListRequirementSurveysCommand,
    LocalRequirementSurvey, LocalRequirementSurveyQuestion, LocalRequirementSurveyResolution,
    LocalRequirementSurveyResponseKind, LocalRequirementSurveyStatus,
    ResolveRequirementSurveyCommand, LOCAL_REQUIREMENT_SURVEY_CREATE_TOOL_NAME,
    LOCAL_REQUIREMENT_SURVEY_MAX_LIST_LIMIT, LOCAL_REQUIREMENT_SURVEY_MAX_QUESTIONS,
};
pub use response::{HostError, HostResponseEnvelope, HostResult};
pub use run_query::{
    ListRunsCommand, LocalAgentRunListScope, LocalAgentRunPage, LocalAgentRunSummary,
};

pub use task::{
    CancelTaskCommand, CreateTaskGraphCommand, GetMessageTaskGraphCommand, GetTaskGraphCommand,
    GetTaskRunsCommand, ListTaskGraphsCommand, LocalMessageTaskGraph, LocalMessageTaskGraphEdge,
    LocalMessageTaskGraphNode, LocalTaskDependency, LocalTaskGraph, LocalTaskGraphListScope,
    LocalTaskGraphPage, LocalTaskGraphStatus, LocalTaskGraphSummary, LocalTaskRecord,
    LocalTaskSpec, LocalTaskStatus, RestartTaskCommand, RetryTaskCommand,
};
pub use tool::{
    ClaimNextToolCommand, CommitToolCommand, DecideToolApprovalCommand,
    ListPendingToolApprovalsCommand, LocalAgentToolApprovalDecision, LocalAgentToolApprovalResult,
    LocalAgentToolApprovalStatus, LocalAgentToolBatch, LocalAgentToolCall, LocalAgentToolClaim,
    LocalAgentToolCommitResult, LocalAgentToolInvocationRecord, LocalAgentToolOutcome,
    LocalAgentToolStatus, RenewToolClaimCommand,
};

pub const LOCAL_AGENT_PROTOCOL_VERSION: u32 = 40;
pub const LOCAL_AGENT_MAX_FRAME_BYTES: usize = 4 * 1024 * 1024;
pub const LOCAL_AGENT_MAX_INPUT_BYTES: usize = 256 * 1024;
pub const LOCAL_AGENT_MAX_EVENT_PAGE_SIZE: u32 = 500;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HostRequestEnvelope {
    pub protocol_version: u32,
    pub command_id: String,
    pub command: HostCommand,
}

impl HostRequestEnvelope {
    pub fn validate(&self) -> Result<(), String> {
        if self.protocol_version != LOCAL_AGENT_PROTOCOL_VERSION {
            return Err(format!(
                "unsupported protocol version {}; expected {}",
                self.protocol_version, LOCAL_AGENT_PROTOCOL_VERSION
            ));
        }
        validate_identifier("command_id", &self.command_id)?;
        self.command.validate()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum HostCommand {
    Health,
    GetMemorySyncStatus(GetMemorySyncStatusCommand),
    PutModelConfigSnapshot(PutModelConfigSnapshotCommand),
    GetModelConfigSnapshot(GetModelConfigSnapshotCommand),
    PutCapabilityPolicySnapshot(PutCapabilityPolicySnapshotCommand),
    GetCapabilityPolicySnapshot(GetCapabilityPolicySnapshotCommand),
    CreateRun(CreateRunCommand),
    GetRun(GetRunCommand),
    ListRuns(ListRunsCommand),
    ClaimNextRun(ClaimNextRunCommand),
    CommitStep(CommitStepCommand),
    ClaimNextTool(ClaimNextToolCommand),
    RenewToolClaim(RenewToolClaimCommand),
    CommitTool(CommitToolCommand),
    ListPendingToolApprovals(ListPendingToolApprovalsCommand),
    DecideToolApproval(DecideToolApprovalCommand),
    ResumeRun(ResumeRunCommand),
    CancelRun(CancelRunCommand),
    GetEventCursor(GetEventCursorCommand),
    ListEvents(ListEventsCommand),
    WaitEvents(WaitEventsCommand),
    CreateTaskGraph(CreateTaskGraphCommand),
    ListTaskGraphs(ListTaskGraphsCommand),
    GetTaskGraph(GetTaskGraphCommand),
    GetMessageTaskGraph(GetMessageTaskGraphCommand),
    GetTaskRuns(GetTaskRunsCommand),
    CancelTask(CancelTaskCommand),
    RetryTask(RetryTaskCommand),
    RestartTask(RestartTaskCommand),
    PutPluginInstallation(PutPluginInstallationCommand),
    GetPluginInstallation(GetPluginInstallationCommand),
    ListPluginInstallations(ListPluginInstallationsCommand),
    RemovePluginInstallation(RemovePluginInstallationCommand),
    CreateConversation(CreateConversationCommand),
    GetConversation(GetConversationCommand),
    GetConversationHistory(GetConversationHistoryCommand),
    ListConversations(ListConversationsCommand),
    GetConversationRuntimeSettings(GetConversationRuntimeSettingsCommand),
    PutConversationRuntimeSettings(PutConversationRuntimeSettingsCommand),
    StartConversationTurn(StartConversationTurnCommand),
    GuideConversationTurn(GuideConversationTurnCommand),
    ResumeConversationTurn(ResumeConversationTurnCommand),
    CancelConversationTurn(CancelConversationTurnCommand),
    InitializeNotepad(InitializeNotepadCommand),
    ListNotepadFolders(ListNotepadFoldersCommand),
    CreateNotepadFolder(CreateNotepadFolderCommand),
    RenameNotepadFolder(RenameNotepadFolderCommand),
    DeleteNotepadFolder(DeleteNotepadFolderCommand),
    ListNotepadNotes(ListNotepadNotesCommand),
    CreateNotepadNote(CreateNotepadNoteCommand),
    GetNotepadNote(GetNotepadNoteCommand),
    UpdateNotepadNote(UpdateNotepadNoteCommand),
    DeleteNotepadNote(DeleteNotepadNoteCommand),
    PutNotepadImage(PutNotepadImageCommand),
    ListRemoteConnections(ListRemoteConnectionsCommand),
    GetRemoteConnection(GetRemoteConnectionCommand),
    CreateRemoteConnection(CreateRemoteConnectionCommand),
    UpdateRemoteConnection(UpdateRemoteConnectionCommand),
    DeleteRemoteConnection(DeleteRemoteConnectionCommand),
    CreateArtifact(CreateArtifactCommand),
    ListArtifacts(ListArtifactsCommand),
    GetArtifactData(GetArtifactDataCommand),
    DeleteArtifact(DeleteArtifactCommand),
    CreateRequirementSurvey(CreateRequirementSurveyCommand),
    ListRequirementSurveys(ListRequirementSurveysCommand),
    GetRequirementSurvey(GetRequirementSurveyCommand),
    ResolveRequirementSurvey(ResolveRequirementSurveyCommand),
}

impl HostCommand {
    pub fn validate(&self) -> Result<(), String> {
        match self {
            Self::Health => Ok(()),
            Self::GetMemorySyncStatus(command) => command.validate(),
            Self::PutModelConfigSnapshot(command) => command.validate(),
            Self::GetModelConfigSnapshot(command) => command.validate(),
            Self::PutCapabilityPolicySnapshot(command) => command.validate(),
            Self::GetCapabilityPolicySnapshot(command) => command.validate(),
            Self::CreateRun(command) => command.validate(),
            Self::GetRun(command) => command.validate(),
            Self::ListRuns(command) => command.validate(),
            Self::ClaimNextRun(command) => command.validate(),
            Self::CommitStep(command) => command.validate(),
            Self::ClaimNextTool(command) => command.validate(),
            Self::RenewToolClaim(command) => command.validate(),
            Self::CommitTool(command) => command.validate(),
            Self::ListPendingToolApprovals(command) => command.validate(),
            Self::DecideToolApproval(command) => command.validate(),
            Self::ResumeRun(command) => command.validate(),
            Self::CancelRun(command) => command.validate(),
            Self::GetEventCursor(command) => command.validate(),
            Self::ListEvents(command) => command.validate(),
            Self::WaitEvents(command) => command.validate(),
            Self::CreateTaskGraph(command) => command.validate(),
            Self::ListTaskGraphs(command) => command.validate(),
            Self::GetTaskGraph(command) => command.validate(),
            Self::GetMessageTaskGraph(command) => command.validate(),
            Self::GetTaskRuns(command) => command.validate(),
            Self::CancelTask(command) => command.validate(),
            Self::RetryTask(command) => command.validate(),
            Self::RestartTask(command) => command.validate(),
            Self::PutPluginInstallation(command) => command.validate(),
            Self::GetPluginInstallation(command) => command.validate(),
            Self::ListPluginInstallations(command) => command.validate(),
            Self::RemovePluginInstallation(command) => command.validate(),
            Self::CreateConversation(command) => command.validate(),
            Self::GetConversation(command) => command.validate(),
            Self::GetConversationHistory(command) => command.validate(),
            Self::ListConversations(command) => command.validate(),
            Self::GetConversationRuntimeSettings(command) => command.validate(),
            Self::PutConversationRuntimeSettings(command) => command.validate(),
            Self::StartConversationTurn(command) => command.validate(),
            Self::GuideConversationTurn(command) => command.validate(),
            Self::ResumeConversationTurn(command) => command.validate(),
            Self::CancelConversationTurn(command) => command.validate(),
            Self::InitializeNotepad(command) => command.validate(),
            Self::ListNotepadFolders(command) => command.validate(),
            Self::CreateNotepadFolder(command) => command.validate(),
            Self::RenameNotepadFolder(command) => command.validate(),
            Self::DeleteNotepadFolder(command) => command.validate(),
            Self::ListNotepadNotes(command) => command.validate(),
            Self::CreateNotepadNote(command) => command.validate(),
            Self::GetNotepadNote(command) => command.validate(),
            Self::UpdateNotepadNote(command) => command.validate(),
            Self::DeleteNotepadNote(command) => command.validate(),
            Self::PutNotepadImage(command) => command.validate(),
            Self::ListRemoteConnections(command) => command.validate(),
            Self::GetRemoteConnection(command) => command.validate(),
            Self::CreateRemoteConnection(command) => command.validate(),
            Self::UpdateRemoteConnection(command) => command.validate(),
            Self::DeleteRemoteConnection(command) => command.validate(),
            Self::CreateArtifact(command) => command.validate(),
            Self::ListArtifacts(command) => command.validate(),
            Self::GetArtifactData(command) => command.validate(),
            Self::DeleteArtifact(command) => command.validate(),
            Self::CreateRequirementSurvey(command) => command.validate(),
            Self::ListRequirementSurveys(command) => command.validate(),
            Self::GetRequirementSurvey(command) => command.validate(),
            Self::ResolveRequirementSurvey(command) => command.validate(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CreateRunCommand {
    pub run_id: String,
    pub owner_user_id: String,
    pub owner_entity_type: String,
    pub owner_entity_id: String,
    pub profile_key: String,
    pub model_config_ref: String,
    pub model_config_revision: String,
    pub capability_policy_revision: String,
    #[serde(default)]
    pub input: Value,
    pub max_iterations: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GetRunCommand {
    pub owner_user_id: String,
    pub run_id: String,
}

impl GetRunCommand {
    pub fn validate(&self) -> Result<(), String> {
        validate_identifier("owner_user_id", &self.owner_user_id)?;
        validate_identifier("run_id", &self.run_id)
    }
}

impl CreateRunCommand {
    pub fn validate(&self) -> Result<(), String> {
        validate_identifier("run_id", &self.run_id)?;
        validate_identifier("owner_user_id", &self.owner_user_id)?;
        validate_identifier("owner_entity_type", &self.owner_entity_type)?;
        validate_identifier("owner_entity_id", &self.owner_entity_id)?;
        validate_identifier("profile_key", &self.profile_key)?;
        validate_identifier("model_config_ref", &self.model_config_ref)?;
        validate_identifier("model_config_revision", &self.model_config_revision)?;
        validate_identifier(
            "capability_policy_revision",
            &self.capability_policy_revision,
        )?;
        if self.max_iterations == 0 {
            return Err("max_iterations must be greater than zero".to_string());
        }
        let input_size = serde_json::to_vec(&self.input)
            .map_err(|error| format!("input is not serializable: {error}"))?
            .len();
        if input_size > LOCAL_AGENT_MAX_INPUT_BYTES {
            return Err(format!(
                "input exceeds the {} byte limit",
                LOCAL_AGENT_MAX_INPUT_BYTES
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ClaimNextRunCommand {
    pub owner_user_id: String,
    pub worker_id: String,
    pub lease_duration_ms: u64,
}

impl ClaimNextRunCommand {
    pub fn validate(&self) -> Result<(), String> {
        validate_identifier("owner_user_id", &self.owner_user_id)?;
        validate_identifier("worker_id", &self.worker_id)?;
        if !(1_000..=300_000).contains(&self.lease_duration_ms) {
            return Err("lease_duration_ms must be between 1000 and 300000".to_string());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CommitStepCommand {
    pub owner_user_id: String,
    pub run_id: String,
    pub claim_token: String,
    pub expected_version: u64,
    pub outcome: LocalAgentStepOutcome,
}

impl CommitStepCommand {
    pub fn validate(&self) -> Result<(), String> {
        validate_identifier("owner_user_id", &self.owner_user_id)?;
        validate_identifier("run_id", &self.run_id)?;
        validate_identifier("claim_token", &self.claim_token)?;
        if self.expected_version == 0 {
            return Err("expected_version must be greater than zero".to_string());
        }
        self.outcome.validate()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CancelRunCommand {
    pub owner_user_id: String,
    pub run_id: String,
    pub expected_version: Option<u64>,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ResumeRunCommand {
    pub owner_user_id: String,
    pub run_id: String,
    pub expected_version: u64,
    pub expected_status: LocalAgentRunStatus,
    pub reason: String,
    #[serde(default)]
    pub input: Value,
}

impl ResumeRunCommand {
    pub fn validate(&self) -> Result<(), String> {
        validate_identifier("owner_user_id", &self.owner_user_id)?;
        validate_identifier("run_id", &self.run_id)?;
        if self.expected_version == 0 {
            return Err("expected_version must be greater than zero".to_string());
        }
        if !matches!(
            self.expected_status,
            LocalAgentRunStatus::WaitingUser
                | LocalAgentRunStatus::Paused
                | LocalAgentRunStatus::NeedsReview
        ) {
            return Err(
                "only waiting_user, paused, or needs_review runs can be resumed".to_string(),
            );
        }
        validate_text("reason", &self.reason, 4_000)?;
        let input_size = serde_json::to_vec(&self.input)
            .map_err(|error| format!("resume input is not serializable: {error}"))?
            .len();
        if input_size > LOCAL_AGENT_MAX_INPUT_BYTES {
            return Err(format!(
                "resume input exceeds the {LOCAL_AGENT_MAX_INPUT_BYTES} byte limit"
            ));
        }
        Ok(())
    }
}

impl CancelRunCommand {
    pub fn validate(&self) -> Result<(), String> {
        validate_identifier("owner_user_id", &self.owner_user_id)?;
        validate_identifier("run_id", &self.run_id)?;
        if self.expected_version == Some(0) {
            return Err("expected_version must be greater than zero".to_string());
        }
        validate_text("reason", &self.reason, 4_000)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GetEventCursorCommand {
    pub owner_user_id: String,
}

impl GetEventCursorCommand {
    pub fn validate(&self) -> Result<(), String> {
        validate_identifier("owner_user_id", &self.owner_user_id)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ListEventsCommand {
    pub owner_user_id: String,
    pub after_cursor: i64,
    pub limit: u32,
    pub run_id: Option<String>,
    #[serde(default)]
    pub event_type: Option<String>,
    #[serde(default)]
    pub newest_first: bool,
    #[serde(default)]
    pub payload_mode: LocalAgentEventPayloadMode,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WaitEventsCommand {
    pub owner_user_id: String,
    pub after_cursor: i64,
    pub limit: u32,
    pub run_id: Option<String>,
    pub timeout_ms: u64,
    #[serde(default)]
    pub payload_mode: LocalAgentEventPayloadMode,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LocalAgentEventPayloadMode {
    #[default]
    Full,
    Routing,
    None,
}

impl WaitEventsCommand {
    pub fn validate(&self) -> Result<(), String> {
        ListEventsCommand {
            owner_user_id: self.owner_user_id.clone(),
            after_cursor: self.after_cursor,
            limit: self.limit,
            run_id: self.run_id.clone(),
            event_type: None,
            newest_first: false,
            payload_mode: self.payload_mode,
        }
        .validate()?;
        if self.timeout_ms == 0 || self.timeout_ms > 60_000 {
            return Err("timeout_ms must be between 1 and 60000".to_string());
        }
        Ok(())
    }
}

impl ListEventsCommand {
    pub fn validate(&self) -> Result<(), String> {
        validate_identifier("owner_user_id", &self.owner_user_id)?;
        if self.after_cursor < 0 {
            return Err("after_cursor must not be negative".to_string());
        }
        if self.limit == 0 || self.limit > LOCAL_AGENT_MAX_EVENT_PAGE_SIZE {
            return Err(format!(
                "limit must be between 1 and {LOCAL_AGENT_MAX_EVENT_PAGE_SIZE}"
            ));
        }
        if let Some(run_id) = self.run_id.as_deref() {
            validate_identifier("run_id", run_id)?;
        }
        if let Some(event_type) = self.event_type.as_deref() {
            validate_identifier("event_type", event_type)?;
            if self.run_id.is_none() {
                return Err("event_type requires run_id".to_string());
            }
        }
        if self.newest_first && (self.run_id.is_none() || self.after_cursor != 0 || self.limit != 1)
        {
            return Err("newest_first requires run_id, after_cursor 0, and limit 1".to_string());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum LocalAgentStepOutcome {
    Continue {
        #[serde(default)]
        checkpoint: Value,
    },
    WaitForTool {
        batch_id: String,
        #[serde(default)]
        tool_calls: Vec<LocalAgentToolCall>,
        #[serde(default)]
        checkpoint: Value,
    },
    WaitForUser {
        #[serde(default)]
        prompt: Value,
        #[serde(default)]
        checkpoint: Value,
    },
    Retry {
        resume_at_unix_ms: i64,
        next_model_attempt: u32,
        reason: String,
    },
    Pause {
        reason: String,
    },
    NeedsReview {
        reason: String,
        #[serde(default)]
        detail: Value,
    },
    Succeed {
        #[serde(default)]
        output: Value,
    },
    Fail {
        error: String,
        #[serde(default)]
        detail: Value,
    },
}

impl LocalAgentStepOutcome {
    pub fn validate(&self) -> Result<(), String> {
        match self {
            Self::Continue { .. } | Self::WaitForUser { .. } | Self::Succeed { .. } => Ok(()),
            Self::WaitForTool {
                batch_id,
                tool_calls,
                ..
            } => {
                validate_identifier("batch_id", batch_id)?;
                if tool_calls.is_empty() {
                    return Err("wait_for_tool requires at least one tool call".to_string());
                }
                LocalAgentToolBatch {
                    batch_id: batch_id.clone(),
                    calls: tool_calls.clone(),
                }
                .validate()?;
                Ok(())
            }
            Self::Retry {
                resume_at_unix_ms,
                next_model_attempt,
                reason,
            } => {
                if *resume_at_unix_ms <= 0 {
                    return Err("resume_at_unix_ms must be positive".to_string());
                }
                if *next_model_attempt < 2 {
                    return Err("next_model_attempt must be at least 2".to_string());
                }
                validate_text("reason", reason, 4_000)
            }
            Self::Pause { reason } | Self::NeedsReview { reason, .. } => {
                validate_text("reason", reason, 4_000)
            }
            Self::Fail { error, .. } => validate_text("error", error, 8_000),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LocalAgentRunStatus {
    Queued,
    ModelReady,
    ModelRunning,
    WaitingToolResult,
    ContinuationReady,
    WaitingUser,
    RetryScheduled,
    Paused,
    NeedsReview,
    Succeeded,
    Failed,
    Cancelled,
}

impl LocalAgentRunStatus {
    pub const fn is_terminal(self) -> bool {
        matches!(self, Self::Succeeded | Self::Failed | Self::Cancelled)
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::ModelReady => "model_ready",
            Self::ModelRunning => "model_running",
            Self::WaitingToolResult => "waiting_tool_result",
            Self::ContinuationReady => "continuation_ready",
            Self::WaitingUser => "waiting_user",
            Self::RetryScheduled => "retry_scheduled",
            Self::Paused => "paused",
            Self::NeedsReview => "needs_review",
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        }
    }
}

impl fmt::Display for LocalAgentRunStatus {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for LocalAgentRunStatus {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "queued" => Ok(Self::Queued),
            "model_ready" => Ok(Self::ModelReady),
            "model_running" => Ok(Self::ModelRunning),
            "waiting_tool_result" => Ok(Self::WaitingToolResult),
            "continuation_ready" => Ok(Self::ContinuationReady),
            "waiting_user" => Ok(Self::WaitingUser),
            "retry_scheduled" => Ok(Self::RetryScheduled),
            "paused" => Ok(Self::Paused),
            "needs_review" => Ok(Self::NeedsReview),
            "succeeded" => Ok(Self::Succeeded),
            "failed" => Ok(Self::Failed),
            "cancelled" => Ok(Self::Cancelled),
            _ => Err(format!("unknown Local Agent run status: {value}")),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LocalAgentRunRecord {
    pub run_id: String,
    pub owner_user_id: String,
    pub owner_entity_type: String,
    pub owner_entity_id: String,
    pub profile_key: String,
    pub model_config_ref: String,
    pub model_config_revision: String,
    pub capability_policy_revision: String,
    pub input: Value,
    pub status: LocalAgentRunStatus,
    pub iteration: u32,
    pub model_attempt: u32,
    pub max_iterations: u32,
    pub version: u64,
    pub claim_token: Option<String>,
    pub claim_until_unix_ms: Option<i64>,
    pub next_attempt_at_unix_ms: Option<i64>,
    pub pending_tool_batch: Option<Value>,
    pub checkpoint: Value,
    pub continuation_input: Option<Value>,
    pub terminal_outcome: Option<Value>,
    pub created_at_unix_ms: i64,
    pub updated_at_unix_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LocalAgentRunClaim {
    pub worker_id: String,
    pub claim_token: String,
    pub run: LocalAgentRunRecord,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LocalAgentEventRecord {
    pub cursor: i64,
    pub event_id: String,
    pub run_id: String,
    pub event_type: String,
    pub payload: Value,
    pub created_at_unix_ms: i64,
}

pub fn validate_identifier(name: &str, value: &str) -> Result<(), String> {
    let value = value.trim();
    if value.is_empty() || value.len() > 256 || value.chars().any(char::is_control) {
        return Err(format!("{name} must be 1..=256 non-control characters"));
    }
    Ok(())
}

pub fn validate_text(name: &str, value: &str, maximum_length: usize) -> Result<(), String> {
    let value = value.trim();
    if value.is_empty() || value.len() > maximum_length || value.chars().any(|value| value == '\0')
    {
        return Err(format!(
            "{name} must be 1..={maximum_length} characters without NUL"
        ));
    }
    Ok(())
}

#[cfg(test)]
mod protocol_tests;
