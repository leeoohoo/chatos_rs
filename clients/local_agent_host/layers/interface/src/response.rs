// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::{
    LocalAgentArtifact, LocalAgentArtifactPage, LocalAgentEventRecord, LocalAgentRunClaim,
    LocalAgentRunPage, LocalAgentRunRecord, LocalAgentToolApprovalResult, LocalAgentToolClaim,
    LocalAgentToolCommitResult, LocalAgentToolInvocationRecord, LocalCapabilityPolicySnapshot,
    LocalConversationDetail, LocalConversationHistoryPage, LocalConversationPage,
    LocalConversationRuntimeSettings, LocalConversationTurnStart, LocalConversationTurnUpdate,
    LocalMemorySyncStatus, LocalMessageTaskGraph, LocalModelConfigSnapshot, LocalNotepadImage,
    LocalNotepadNote, LocalNotepadNoteDetail, LocalPluginInstallationPage,
    LocalPluginInstallationRecord, LocalRemoteConnection, LocalRequirementSurvey,
    LocalRequirementSurveyResolution, LocalTaskGraph, LocalTaskGraphPage,
    LOCAL_AGENT_PROTOCOL_VERSION,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HostResponseEnvelope {
    pub protocol_version: u32,
    pub command_id: String,
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<HostResult>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<HostError>,
}

impl HostResponseEnvelope {
    pub fn success(command_id: String, result: HostResult) -> Self {
        Self {
            protocol_version: LOCAL_AGENT_PROTOCOL_VERSION,
            command_id,
            ok: true,
            result: Some(result),
            error: None,
        }
    }

    pub fn failure(command_id: String, error: HostError) -> Self {
        Self {
            protocol_version: LOCAL_AGENT_PROTOCOL_VERSION,
            command_id,
            ok: false,
            result: None,
            error: Some(error),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum HostResult {
    Health {
        service: String,
        storage_ready: bool,
        recovered_claims: u64,
    },
    MemorySyncStatus {
        status: LocalMemorySyncStatus,
    },
    ModelConfigSnapshot {
        snapshot: LocalModelConfigSnapshot,
    },
    ModelConfigSnapshots {
        snapshots: Vec<LocalModelConfigSnapshot>,
    },
    CapabilityPolicySnapshot {
        snapshot: LocalCapabilityPolicySnapshot,
    },
    Run {
        run: LocalAgentRunRecord,
    },
    Runs {
        page: LocalAgentRunPage,
    },
    Claim {
        claim: Option<LocalAgentRunClaim>,
    },
    ToolClaim {
        claim: Option<LocalAgentToolClaim>,
    },
    ToolClaimRenewed {
        renewed: bool,
    },
    ToolCommit {
        result: Box<LocalAgentToolCommitResult>,
    },
    PendingToolApprovals {
        invocations: Vec<LocalAgentToolInvocationRecord>,
    },
    ToolApproval {
        result: Box<LocalAgentToolApprovalResult>,
    },
    Events {
        events: Vec<LocalAgentEventRecord>,
        next_cursor: i64,
    },
    EventCursor {
        cursor: i64,
    },
    TaskGraph {
        graph: LocalTaskGraph,
    },
    TaskGraphs {
        page: LocalTaskGraphPage,
    },
    MessageTaskGraph {
        graph: LocalMessageTaskGraph,
    },
    TaskRuns {
        task_id: String,
        runs: Vec<LocalAgentRunRecord>,
    },
    PluginInstallation {
        installation: LocalPluginInstallationRecord,
    },
    PluginInstallations {
        page: LocalPluginInstallationPage,
    },
    Conversation {
        conversation: LocalConversationDetail,
    },
    Conversations {
        page: LocalConversationPage,
    },
    ConversationHistory {
        page: Box<LocalConversationHistoryPage>,
    },
    ConversationRuntimeSettings {
        settings: LocalConversationRuntimeSettings,
    },
    ConversationTurnStarted {
        result: Box<LocalConversationTurnStart>,
    },
    ConversationTurnUpdated {
        result: Box<LocalConversationTurnUpdate>,
    },
    NotepadInitialized {
        note_count: u64,
    },
    NotepadFolders {
        folders: Vec<String>,
    },
    NotepadFolderMutation {
        folder: String,
        affected_notes: u64,
    },
    NotepadNotes {
        notes: Vec<LocalNotepadNote>,
    },
    NotepadNote {
        detail: LocalNotepadNoteDetail,
    },
    NotepadNoteDeleted {
        note_id: String,
    },
    NotepadImage {
        image: LocalNotepadImage,
    },
    RemoteConnections {
        connections: Vec<LocalRemoteConnection>,
    },
    RemoteConnection {
        connection: Option<LocalRemoteConnection>,
    },
    RemoteConnectionDeleted {
        connection_id: String,
    },
    Artifact {
        artifact: LocalAgentArtifact,
    },
    Artifacts {
        page: LocalAgentArtifactPage,
    },
    ArtifactData {
        artifact_id: String,
        data_base64: String,
    },
    ArtifactDeleted {
        artifact_id: String,
    },
    RequirementSurvey {
        survey: LocalRequirementSurvey,
    },
    RequirementSurveys {
        surveys: Vec<LocalRequirementSurvey>,
    },
    RequirementSurveyResolved {
        resolution: Box<LocalRequirementSurveyResolution>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HostError {
    pub code: String,
    pub message: String,
    pub retryable: bool,
}

impl HostError {
    pub fn new(code: impl Into<String>, message: impl Into<String>, retryable: bool) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            retryable,
        }
    }
}
