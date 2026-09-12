// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chrono::{DateTime, Utc};
use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::{
    require_bounded_json, require_digest, require_identifier, AgentMessage, AgentMessageRole,
    ApplyStorageProfileCommand, ClientDataTransferResult, ClientStorageProfileDescriptor,
    ExportClientDataCommand, ImportClientDataCommand, InstallProjectPluginCapabilityCommand,
    LocalAgentRun, PostgresConnectionTestCommand, PostgresConnectionTestResult, ProtocolError,
    RemoveProjectPluginCapabilityCommand, ToolExecution, LOCAL_AGENT_PROTOCOL_VERSION,
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct LocalAgentIpcRequest {
    pub protocol_version: u32,
    pub request_id: String,
    pub owner_user_id: String,
    pub command: LocalAgentCommand,
}

impl LocalAgentIpcRequest {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        validate_protocol_version(self.protocol_version)?;
        require_identifier("request_id", &self.request_id)?;
        require_identifier("owner_user_id", &self.owner_user_id)?;
        self.command.validate()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct LocalAgentIpcReply {
    pub protocol_version: u32,
    pub request_id: String,
    pub response: LocalAgentIpcResponse,
}

impl LocalAgentIpcReply {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        validate_protocol_version(self.protocol_version)?;
        require_identifier("request_id", &self.request_id)?;
        self.response.validate()
    }
}

fn validate_protocol_version(protocol_version: u32) -> Result<(), ProtocolError> {
    if protocol_version == LOCAL_AGENT_PROTOCOL_VERSION {
        Ok(())
    } else {
        Err(ProtocolError::InvalidState {
            reason: "unsupported local Agent protocol version",
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", content = "payload", rename_all = "snake_case")]
pub enum LocalAgentCommand {
    CreateMainChatTurn(Box<CreateMainChatTurnCommand>),
    CreateTask(Box<CreateTaskCommand>),
    RetryTask(RetryTaskCommand),
    PauseRun(RunControlCommand),
    ResumeRun(RunControlCommand),
    CancelRun(RunControlCommand),
    AnswerUserQuestion(AnswerUserQuestionCommand),
    DecideToolApproval(ToolApprovalCommand),
    GetRun { run_id: String },
    GetRunDetail(GetRunDetailCommand),
    GetTask { task_id: String },
    GetTaskGraph(GetTaskGraphCommand),
    GetTaskRunDetail(GetTaskRunDetailCommand),
    GetMainChatRunBinding { run_id: String },
    ListRuns { cursor: Option<String>, limit: u32 },
    ListTasks { cursor: Option<String>, limit: u32 },
    SubscribeRunEvents { after_seq: u64, limit: u32 },
    GetUiEventCursor,
    AcknowledgeUiEvents { through_seq: u64 },
    GetStorageProfile,
    TestPostgresConnection(PostgresConnectionTestCommand),
    ApplyStorageProfile(ApplyStorageProfileCommand),
    ExportClientData(ExportClientDataCommand),
    ImportClientData(ImportClientDataCommand),
    InstallProjectPluginCapability(InstallProjectPluginCapabilityCommand),
    RemoveProjectPluginCapability(RemoveProjectPluginCapabilityCommand),
}

impl LocalAgentCommand {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        match self {
            Self::CreateMainChatTurn(command) => command.validate(),
            Self::CreateTask(command) => command.validate(),
            Self::RetryTask(command) => command.validate(),
            Self::PauseRun(command) | Self::ResumeRun(command) | Self::CancelRun(command) => {
                command.validate()
            }
            Self::GetRun { run_id } | Self::GetMainChatRunBinding { run_id } => {
                require_identifier("run_id", run_id)
            }
            Self::GetTask { task_id } => require_identifier("task_id", task_id),
            Self::GetRunDetail(command) => command.validate(),
            Self::GetTaskGraph(command) => command.validate(),
            Self::GetTaskRunDetail(command) => command.validate(),
            Self::AnswerUserQuestion(command) => command.validate(),
            Self::DecideToolApproval(command) => command.validate(),
            Self::ListRuns { cursor, limit } | Self::ListTasks { cursor, limit } => {
                validate_page(cursor.as_deref(), *limit)
            }
            Self::SubscribeRunEvents { limit, .. } => validate_page(None, *limit),
            Self::GetUiEventCursor => Ok(()),
            Self::AcknowledgeUiEvents { through_seq } => {
                if *through_seq == 0 {
                    Err(ProtocolError::InvalidState {
                        reason: "acknowledged UI event sequence must be positive",
                    })
                } else {
                    Ok(())
                }
            }
            Self::GetStorageProfile => Ok(()),
            Self::TestPostgresConnection(command) => command.validate(),
            Self::ApplyStorageProfile(command) => command.validate(),
            Self::ExportClientData(command) => command.validate(),
            Self::ImportClientData(command) => command.validate(),
            Self::InstallProjectPluginCapability(command) => command.validate(),
            Self::RemoveProjectPluginCapability(command) => command.validate(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct GetRunDetailCommand {
    pub run_id: String,
    pub event_limit: u32,
    pub event_offset: u32,
}

impl GetRunDetailCommand {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        require_identifier("run_id", &self.run_id)?;
        validate_page(None, self.event_limit)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct GetTaskGraphCommand {
    pub source_thread_id: String,
    pub source_turn_id: String,
}

impl GetTaskGraphCommand {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        require_identifier("source_thread_id", &self.source_thread_id)?;
        require_identifier("source_turn_id", &self.source_turn_id)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct GetTaskRunDetailCommand {
    pub task_id: String,
    pub run_id: String,
    pub event_limit: u32,
    pub event_offset: u32,
}

impl GetTaskRunDetailCommand {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        require_identifier("task_id", &self.task_id)?;
        require_identifier("run_id", &self.run_id)?;
        validate_page(None, self.event_limit)
    }
}

fn validate_page(cursor: Option<&str>, limit: u32) -> Result<(), ProtocolError> {
    if let Some(cursor) = cursor {
        require_identifier("cursor", cursor)?;
    }
    if limit == 0 || limit > 500 {
        return Err(ProtocolError::InvalidState {
            reason: "IPC page limit must be between 1 and 500",
        });
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct CreateMainChatTurnCommand {
    pub thread_id: String,
    pub turn_id: String,
    pub message_id: String,
    pub project_id: Option<String>,
    pub model_config_id: String,
    pub prompt_snapshot: FrozenSnapshot,
    pub capability_snapshot: FrozenSnapshot,
    pub project_snapshot: Option<FrozenSnapshot>,
    pub content: Option<String>,
    pub attachments: Vec<LocalAttachmentReference>,
}

impl CreateMainChatTurnCommand {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        for (field, value) in [
            ("thread_id", self.thread_id.as_str()),
            ("turn_id", self.turn_id.as_str()),
            ("message_id", self.message_id.as_str()),
            ("model_config_id", self.model_config_id.as_str()),
        ] {
            require_identifier(field, value)?;
        }
        if let Some(project_id) = &self.project_id {
            require_identifier("project_id", project_id)?;
        }
        if self.content.as_deref().is_none_or(str::is_empty) && self.attachments.is_empty() {
            return Err(ProtocolError::InvalidState {
                reason: "main chat turn requires content or an attachment",
            });
        }
        let mut attachment_ids = BTreeSet::new();
        for attachment in &self.attachments {
            attachment.validate()?;
            if !attachment_ids.insert(attachment.attachment_id.as_str()) {
                return Err(ProtocolError::InvalidState {
                    reason: "main chat attachment IDs must be unique",
                });
            }
        }
        self.prompt_snapshot.validate("prompt_snapshot")?;
        self.capability_snapshot.validate("capability_snapshot")?;
        match (&self.project_id, &self.project_snapshot) {
            (Some(_), Some(snapshot)) => snapshot.validate("project_snapshot")?,
            (None, None) => {}
            _ => {
                return Err(ProtocolError::InvalidState {
                    reason: "main chat project_id and project_snapshot must be supplied together",
                });
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct LocalAttachmentReference {
    pub attachment_id: String,
    pub media_type: String,
    pub payload_reference: String,
    pub payload_digest: String,
    pub byte_size: u64,
}

impl LocalAttachmentReference {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        require_identifier("attachment_id", &self.attachment_id)?;
        require_identifier("media_type", &self.media_type)?;
        require_identifier("payload_reference", &self.payload_reference)?;
        require_digest("payload_digest", &self.payload_digest)?;
        if self.payload_reference.starts_with('/')
            || self.payload_reference.starts_with("file://")
            || (self.payload_reference.len() >= 3
                && self.payload_reference.as_bytes()[0].is_ascii_alphabetic()
                && self.payload_reference.as_bytes()[1] == b':'
                && matches!(self.payload_reference.as_bytes()[2], b'\\' | b'/'))
        {
            return Err(ProtocolError::InvalidState {
                reason: "attachment payload_reference must be an opaque local grant",
            });
        }
        if self.byte_size == 0 {
            return Err(ProtocolError::InvalidState {
                reason: "attachment byte size must be positive",
            });
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct CreateTaskCommand {
    pub task_id: String,
    pub source_thread_id: String,
    pub source_turn_id: String,
    pub project_id: String,
    pub objective: String,
    pub acceptance_criteria: Vec<String>,
    pub model_config_id: String,
    pub prompt_snapshot: FrozenSnapshot,
    pub project_snapshot: FrozenSnapshot,
    pub capability_snapshot: FrozenSnapshot,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RetryTaskCommand {
    pub task_id: String,
    pub expected_run_id: String,
    pub instruction: Option<String>,
}

impl RetryTaskCommand {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        require_identifier("task_id", &self.task_id)?;
        require_identifier("expected_run_id", &self.expected_run_id)?;
        if self
            .instruction
            .as_deref()
            .is_some_and(|instruction| instruction.trim().is_empty())
        {
            return Err(ProtocolError::EmptyPayload {
                field: "retry_instruction",
            });
        }
        Ok(())
    }
}

impl CreateTaskCommand {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        for (field, value) in [
            ("task_id", self.task_id.as_str()),
            ("source_thread_id", self.source_thread_id.as_str()),
            ("source_turn_id", self.source_turn_id.as_str()),
            ("project_id", self.project_id.as_str()),
            ("model_config_id", self.model_config_id.as_str()),
        ] {
            require_identifier(field, value)?;
        }
        if self.objective.trim().is_empty() {
            return Err(ProtocolError::EmptyPayload {
                field: "task_objective",
            });
        }
        if self.acceptance_criteria.is_empty()
            || self
                .acceptance_criteria
                .iter()
                .any(|criterion| criterion.trim().is_empty())
        {
            return Err(ProtocolError::InvalidState {
                reason: "task acceptance criteria must be non-empty",
            });
        }
        self.prompt_snapshot.validate("prompt_snapshot")?;
        self.project_snapshot.validate("project_snapshot")?;
        self.capability_snapshot.validate("capability_snapshot")
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct FrozenSnapshot {
    pub snapshot_id: String,
    pub revision: String,
    pub digest: String,
    pub payload: Value,
}

impl FrozenSnapshot {
    pub fn new(
        snapshot_id: impl Into<String>,
        revision: impl Into<String>,
        payload: Value,
    ) -> Result<Self, ProtocolError> {
        let snapshot = Self {
            snapshot_id: snapshot_id.into(),
            revision: revision.into(),
            digest: snapshot_payload_digest(&payload),
            payload,
        };
        snapshot.validate("frozen_snapshot")?;
        Ok(snapshot)
    }

    pub fn validate(&self, field: &'static str) -> Result<(), ProtocolError> {
        require_identifier(field, &self.snapshot_id)?;
        require_identifier("snapshot_revision", &self.revision)?;
        require_digest("snapshot_digest", &self.digest)?;
        require_bounded_json("snapshot_payload", &self.payload)?;
        if !self.payload.is_object() {
            return Err(ProtocolError::InvalidState {
                reason: "frozen snapshot payload must be an object",
            });
        }
        if self.digest != snapshot_payload_digest(&self.payload) {
            return Err(ProtocolError::InvalidDigest {
                field: "snapshot_digest",
            });
        }
        Ok(())
    }
}

fn snapshot_payload_digest(payload: &Value) -> String {
    let bytes = serde_json::to_vec(&canonicalize_snapshot_payload(payload))
        .expect("serde_json::Value is always serializable");
    format!("sha256:{:x}", Sha256::digest(bytes))
}

fn canonicalize_snapshot_payload(value: &Value) -> Value {
    match value {
        Value::Array(values) => {
            Value::Array(values.iter().map(canonicalize_snapshot_payload).collect())
        }
        Value::Object(values) => Value::Object(
            values
                .iter()
                .map(|(key, value)| (key.clone(), canonicalize_snapshot_payload(value)))
                .collect::<BTreeMap<_, _>>()
                .into_iter()
                .collect(),
        ),
        value => value.clone(),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct AnswerUserQuestionCommand {
    pub run_id: String,
    pub interaction_id: String,
    pub answer: UserInteractionAnswer,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RunControlCommand {
    pub run_id: String,
    pub expected_version: u64,
}

impl RunControlCommand {
    fn validate(&self) -> Result<(), ProtocolError> {
        require_identifier("run_id", &self.run_id)?;
        if self.expected_version == 0 {
            return Err(ProtocolError::InvalidState {
                reason: "Run control expected version must be positive",
            });
        }
        Ok(())
    }
}

impl AnswerUserQuestionCommand {
    fn validate(&self) -> Result<(), ProtocolError> {
        require_identifier("run_id", &self.run_id)?;
        require_identifier("interaction_id", &self.interaction_id)?;
        self.answer.validate()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct UserInteractionAnswer {
    pub text: Option<String>,
    pub selected_option_ids: Vec<String>,
    pub attachments: Vec<LocalAttachmentReference>,
}

impl UserInteractionAnswer {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        if self
            .text
            .as_deref()
            .is_none_or(|text| text.trim().is_empty())
            && self.selected_option_ids.is_empty()
            && self.attachments.is_empty()
        {
            return Err(ProtocolError::InvalidState {
                reason: "user interaction answer must not be empty",
            });
        }
        for option_id in &self.selected_option_ids {
            require_identifier("selected_option_id", option_id)?;
        }
        for attachment in &self.attachments {
            attachment.validate()?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ToolApprovalDecision {
    Approve,
    Reject,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ToolApprovalCommand {
    pub run_id: String,
    pub invocation_id: String,
    pub decision: ToolApprovalDecision,
    pub reason: Option<String>,
}

/// Owner-scoped, user-facing Task aggregate restored by native clients.
///
/// The execution state remains authoritative in `LocalAgentRun`; this snapshot
/// exposes only stable Task identity and frozen planning input. Native clients
/// join it to its current and historical Runs by their stable IDs and never
/// reconstruct Task identity from UI
/// events or a remote Task Runner service.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct LocalAgentTaskSnapshot {
    pub task_id: String,
    pub revision: u64,
    pub source_thread_id: String,
    pub source_turn_id: String,
    pub project_id: String,
    pub initial_run_id: String,
    pub current_run_id: String,
    pub run_ids: Vec<String>,
    pub objective: String,
    pub acceptance_criteria: Vec<String>,
    pub status: String,
    pub model_config_id: String,
    pub model_config_revision: u64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl LocalAgentTaskSnapshot {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        for (field, value) in [
            ("task_id", self.task_id.as_str()),
            ("source_thread_id", self.source_thread_id.as_str()),
            ("source_turn_id", self.source_turn_id.as_str()),
            ("project_id", self.project_id.as_str()),
            ("current_run_id", self.current_run_id.as_str()),
            ("model_config_id", self.model_config_id.as_str()),
        ] {
            require_identifier(field, value)?;
        }
        if self.objective.trim().is_empty() {
            return Err(ProtocolError::EmptyPayload {
                field: "task_objective",
            });
        }
        if self.acceptance_criteria.is_empty()
            || self
                .acceptance_criteria
                .iter()
                .any(|criterion| criterion.trim().is_empty())
        {
            return Err(ProtocolError::InvalidState {
                reason: "task acceptance criteria must be non-empty",
            });
        }
        let mut run_ids = BTreeSet::new();
        for run_id in &self.run_ids {
            require_identifier("run_id", run_id)?;
            if !run_ids.insert(run_id.as_str()) {
                return Err(ProtocolError::InvalidState {
                    reason: "task run IDs must be unique",
                });
            }
        }
        require_identifier("initial_run_id", &self.initial_run_id)?;
        if self.run_ids.first() != Some(&self.initial_run_id)
            || !run_ids.contains(self.current_run_id.as_str())
        {
            return Err(ProtocolError::InvalidState {
                reason: "task initial/current Run IDs must match ordered run_ids",
            });
        }
        require_identifier("task_status", &self.status)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct LocalAgentTaskRunSummary {
    pub run: LocalAgentRun,
    pub result_summary: Option<String>,
    pub report_content: Option<String>,
    pub error_message: Option<String>,
}

impl LocalAgentTaskRunSummary {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        self.run.validate()?;
        for value in [
            self.result_summary.as_deref(),
            self.report_content.as_deref(),
            self.error_message.as_deref(),
        ]
        .into_iter()
        .flatten()
        {
            if value.trim().is_empty() {
                return Err(ProtocolError::InvalidState {
                    reason: "Task Run summary text must be non-empty when present",
                });
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct LocalAgentTaskProjection {
    pub task: LocalAgentTaskSnapshot,
    pub current_run: LocalAgentTaskRunSummary,
}

impl LocalAgentTaskProjection {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        self.task.validate()?;
        self.current_run.validate()?;
        if self.task.current_run_id != self.current_run.run.run_id
            || self.current_run.run.owner_entity_type != "task"
            || self.current_run.run.owner_entity_id != self.task.task_id
            || self.current_run.run.project_id.as_deref() != Some(self.task.project_id.as_str())
        {
            return Err(ProtocolError::InvalidState {
                reason: "Task projection identity does not match its current Run",
            });
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct LocalAgentTaskGraphNode {
    pub task: LocalAgentTaskProjection,
    pub depth: u32,
    pub is_root: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct LocalAgentTaskGraphEdge {
    pub edge_id: String,
    pub source_task_id: String,
    pub target_task_id: String,
    pub kind: String,
}

impl LocalAgentTaskGraphEdge {
    fn validate(&self) -> Result<(), ProtocolError> {
        require_identifier("edge_id", &self.edge_id)?;
        require_identifier("source_task_id", &self.source_task_id)?;
        require_identifier("target_task_id", &self.target_task_id)?;
        require_identifier("task_graph_edge_kind", &self.kind)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct LocalAgentTaskGraphSnapshot {
    pub source_thread_id: String,
    pub source_turn_id: String,
    pub root_task_ids: Vec<String>,
    pub nodes: Vec<LocalAgentTaskGraphNode>,
    pub edges: Vec<LocalAgentTaskGraphEdge>,
}

impl LocalAgentTaskGraphSnapshot {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        require_identifier("source_thread_id", &self.source_thread_id)?;
        require_identifier("source_turn_id", &self.source_turn_id)?;
        let mut node_ids = BTreeSet::new();
        for node in &self.nodes {
            node.task.validate()?;
            if node.task.task.source_thread_id != self.source_thread_id
                || node.task.task.source_turn_id != self.source_turn_id
                || !node_ids.insert(node.task.task.task_id.as_str())
            {
                return Err(ProtocolError::InvalidState {
                    reason: "Task Graph nodes must be unique and match the requested source",
                });
            }
        }
        let roots = self.root_task_ids.iter().collect::<BTreeSet<_>>();
        if roots.len() != self.root_task_ids.len()
            || roots
                .iter()
                .any(|task_id| !node_ids.contains(task_id.as_str()))
        {
            return Err(ProtocolError::InvalidState {
                reason: "Task Graph roots must be unique graph nodes",
            });
        }
        for edge in &self.edges {
            edge.validate()?;
            if !node_ids.contains(edge.source_task_id.as_str())
                || !node_ids.contains(edge.target_task_id.as_str())
            {
                return Err(ProtocolError::InvalidState {
                    reason: "Task Graph edge endpoint is not a graph node",
                });
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct LocalAgentRunTimelineEvent {
    pub event_id: String,
    pub event_type: String,
    pub message: Option<String>,
    pub created_at: DateTime<Utc>,
}

impl LocalAgentRunTimelineEvent {
    fn validate(&self) -> Result<(), ProtocolError> {
        require_identifier("event_id", &self.event_id)?;
        require_identifier("event_type", &self.event_type)?;
        if self
            .message
            .as_deref()
            .is_some_and(|value| value.trim().is_empty())
        {
            return Err(ProtocolError::InvalidState {
                reason: "Run timeline event message must be non-empty when present",
            });
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct LocalAgentTaskRunDetail {
    pub task: LocalAgentTaskSnapshot,
    pub run: LocalAgentTaskRunSummary,
    pub events: Vec<LocalAgentRunTimelineEvent>,
    pub events_total: u32,
    pub events_has_more: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct LocalAgentRunDetail {
    pub run: LocalAgentRun,
    pub events: Vec<LocalAgentRunTimelineEvent>,
    pub tools: Vec<crate::ToolExecution>,
    pub events_total: u32,
    pub events_has_more: bool,
    pub snapshot_event_sequence: u64,
}

impl LocalAgentRunDetail {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        self.run.validate()?;
        if self.events_total < self.events.len() as u32 {
            return Err(ProtocolError::InvalidState {
                reason: "Run detail event count is invalid",
            });
        }
        for event in &self.events {
            event.validate()?;
        }
        for tool in &self.tools {
            tool.validate()?;
            if tool.run_id != self.run.run_id {
                return Err(ProtocolError::InvalidState {
                    reason: "Run detail tool identity is invalid",
                });
            }
        }
        Ok(())
    }
}

impl LocalAgentTaskRunDetail {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        self.task.validate()?;
        self.run.validate()?;
        if self.run.run.owner_entity_type != "task"
            || self.run.run.owner_entity_id != self.task.task_id
            || !self.task.run_ids.contains(&self.run.run.run_id)
            || self.run.run.project_id.as_deref() != Some(self.task.project_id.as_str())
            || self.events_total < self.events.len() as u32
        {
            return Err(ProtocolError::InvalidState {
                reason: "Task Run detail identity or event count is invalid",
            });
        }
        for event in &self.events {
            event.validate()?;
        }
        Ok(())
    }
}

impl ToolApprovalCommand {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        require_identifier("run_id", &self.run_id)?;
        require_identifier("invocation_id", &self.invocation_id)?;
        if self
            .reason
            .as_deref()
            .is_some_and(|reason| reason.trim().is_empty())
        {
            return Err(ProtocolError::EmptyPayload {
                field: "approval_reason",
            });
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", content = "payload", rename_all = "snake_case")]
pub enum LocalAgentIpcResponse {
    Accepted {
        operation_id: String,
    },
    RunCreated {
        operation_id: String,
        run: Box<LocalAgentRun>,
    },
    Run(Box<LocalAgentRun>),
    RunDetail(Box<LocalAgentRunDetail>),
    Task(Box<LocalAgentTaskSnapshot>),
    TaskGraph(Box<LocalAgentTaskGraphSnapshot>),
    TaskRunDetail(Box<LocalAgentTaskRunDetail>),
    MainChatRunBinding(MainChatRunBinding),
    Runs {
        runs: Vec<LocalAgentRun>,
        next_cursor: Option<String>,
    },
    Tasks {
        tasks: Vec<LocalAgentTaskSnapshot>,
        next_cursor: Option<String>,
    },
    Events {
        events: Vec<LocalAgentUiEvent>,
        next_seq: u64,
        has_more: bool,
    },
    UiEventCursor {
        event_seq: u64,
    },
    StorageProfile(ClientStorageProfileDescriptor),
    PostgresConnectionTest(PostgresConnectionTestResult),
    DataTransfer(ClientDataTransferResult),
    Success,
    Error(LocalAgentIpcError),
}

impl LocalAgentIpcResponse {
    fn validate(&self) -> Result<(), ProtocolError> {
        match self {
            Self::Accepted { operation_id } => require_identifier("operation_id", operation_id),
            Self::RunCreated { operation_id, run } => {
                require_identifier("operation_id", operation_id)?;
                run.validate()
            }
            Self::Run(run) => run.validate(),
            Self::RunDetail(detail) => detail.validate(),
            Self::Task(task) => task.validate(),
            Self::TaskGraph(graph) => graph.validate(),
            Self::TaskRunDetail(detail) => detail.validate(),
            Self::MainChatRunBinding(binding) => binding.validate(),
            Self::Runs { runs, next_cursor } => {
                for run in runs {
                    run.validate()?;
                }
                if let Some(cursor) = next_cursor {
                    require_identifier("next_cursor", cursor)?;
                }
                Ok(())
            }
            Self::Tasks { tasks, next_cursor } => {
                for task in tasks {
                    task.validate()?;
                }
                if let Some(cursor) = next_cursor {
                    require_identifier("next_cursor", cursor)?;
                }
                Ok(())
            }
            Self::Events {
                events, next_seq, ..
            } => {
                for event in events {
                    event.validate()?;
                    if event.event_seq > *next_seq {
                        return Err(ProtocolError::InvalidState {
                            reason: "event sequence exceeds the response cursor",
                        });
                    }
                }
                Ok(())
            }
            Self::UiEventCursor { .. } => Ok(()),
            Self::StorageProfile(profile) => profile.validate(),
            Self::PostgresConnectionTest(result) => result.validate(),
            Self::DataTransfer(result) => result.validate(),
            Self::Success => Ok(()),
            Self::Error(error) => error.validate(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct MainChatRunBinding {
    pub run_id: String,
    pub thread_id: String,
    pub turn_id: String,
    pub message_id: String,
    pub user_message: Box<AgentMessage>,
}

impl MainChatRunBinding {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        for (field, value) in [
            ("run_id", self.run_id.as_str()),
            ("thread_id", self.thread_id.as_str()),
            ("turn_id", self.turn_id.as_str()),
            ("message_id", self.message_id.as_str()),
        ] {
            require_identifier(field, value)?;
        }
        self.user_message.validate()?;
        if self.user_message.run_id != self.run_id
            || self.user_message.thread_id != self.thread_id
            || self.user_message.turn_id != self.turn_id
            || self.user_message.record_id != self.message_id
            || self.user_message.role != AgentMessageRole::User
            || self.user_message.message_source != "main_chat"
        {
            return Err(ProtocolError::InvalidState {
                reason: "Main Chat binding does not match its initial user message",
            });
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct LocalAgentUiEvent {
    pub event_seq: u64,
    pub emitted_at: DateTime<Utc>,
    pub event: LocalAgentUiEventPayload,
}

impl LocalAgentUiEvent {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        if self.event_seq == 0 {
            return Err(ProtocolError::InvalidState {
                reason: "UI event sequence must be positive",
            });
        }
        self.event.validate()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", content = "payload", rename_all = "snake_case")]
pub enum LocalAgentUiEventPayload {
    RunSnapshot(Box<LocalAgentRun>),
    ModelStream(ModelStreamUiEvent),
    ToolSnapshot(Box<ToolExecution>),
    UserInteraction(UserInteractionRequest),
    MemorySync(MemorySyncUiStatus),
    HostStatus(LocalAgentHostUiStatus),
}

impl LocalAgentUiEventPayload {
    fn validate(&self) -> Result<(), ProtocolError> {
        match self {
            Self::RunSnapshot(run) => run.validate(),
            Self::ModelStream(event) => event.validate(),
            Self::ToolSnapshot(execution) => execution.validate(),
            Self::UserInteraction(interaction) => interaction.validate(),
            Self::MemorySync(status) => status.validate(),
            Self::HostStatus(status) => status.validate(),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ModelStreamDeltaKind {
    Content,
    Reasoning,
    Status,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ModelStreamUiEvent {
    pub run_id: String,
    pub step_seq: u64,
    pub delta_kind: ModelStreamDeltaKind,
    pub delta: String,
}

impl ModelStreamUiEvent {
    fn validate(&self) -> Result<(), ProtocolError> {
        require_identifier("run_id", &self.run_id)?;
        if self.step_seq == 0 || self.delta.is_empty() {
            return Err(ProtocolError::InvalidState {
                reason: "model stream event requires a step and non-empty delta",
            });
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct UserInteractionRequest {
    pub interaction_id: String,
    pub run_id: String,
    pub prompt: String,
    pub options: Vec<UserInteractionOption>,
    pub image_references: Vec<String>,
    pub details: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct UserInteractionQuestion {
    pub prompt: String,
    pub options: Vec<UserInteractionOption>,
    pub image_references: Vec<String>,
    pub details: Option<Value>,
}

impl UserInteractionQuestion {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        if self.prompt.trim().is_empty() {
            return Err(ProtocolError::EmptyPayload {
                field: "interaction_prompt",
            });
        }
        for option in &self.options {
            option.validate()?;
        }
        for image_reference in &self.image_references {
            require_identifier("image_reference", image_reference)?;
        }
        if let Some(details) = &self.details {
            require_bounded_json("interaction_details", details)?;
        }
        Ok(())
    }
}

impl UserInteractionRequest {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        require_identifier("interaction_id", &self.interaction_id)?;
        require_identifier("run_id", &self.run_id)?;
        UserInteractionQuestion {
            prompt: self.prompt.clone(),
            options: self.options.clone(),
            image_references: self.image_references.clone(),
            details: self.details.clone(),
        }
        .validate()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct UserInteractionOption {
    pub option_id: String,
    pub label: String,
    pub description: Option<String>,
}

impl UserInteractionOption {
    fn validate(&self) -> Result<(), ProtocolError> {
        require_identifier("interaction_option_id", &self.option_id)?;
        if self.label.trim().is_empty() {
            return Err(ProtocolError::EmptyPayload {
                field: "interaction_option_label",
            });
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct MemorySyncUiStatus {
    pub run_id: String,
    pub pending_count: u64,
    pub failed_count: u64,
    pub last_error_code: Option<String>,
}

impl MemorySyncUiStatus {
    fn validate(&self) -> Result<(), ProtocolError> {
        require_identifier("run_id", &self.run_id)?;
        if let Some(code) = &self.last_error_code {
            require_identifier("memory_sync_error_code", code)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LocalAgentHostState {
    Starting,
    Ready,
    StorageUnavailable,
    ShuttingDown,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct LocalAgentHostUiStatus {
    pub state: LocalAgentHostState,
    pub active_run_count: u64,
    pub error_code: Option<String>,
}

impl LocalAgentHostUiStatus {
    fn validate(&self) -> Result<(), ProtocolError> {
        if let Some(code) = &self.error_code {
            require_identifier("host_error_code", code)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct LocalAgentIpcError {
    pub code: String,
    pub message: String,
    pub retryable: bool,
}

impl LocalAgentIpcError {
    fn validate(&self) -> Result<(), ProtocolError> {
        require_identifier("ipc_error_code", &self.code)?;
        if self.message.trim().is_empty() {
            return Err(ProtocolError::EmptyPayload {
                field: "ipc_error_message",
            });
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_serialization_uses_stable_snake_case_tags() {
        let value = serde_json::to_value(LocalAgentCommand::PauseRun(RunControlCommand {
            run_id: "run-1".to_string(),
            expected_version: 7,
        }))
        .unwrap();
        assert_eq!(value["type"], "pause_run");
        assert_eq!(value["payload"]["run_id"], "run-1");
        assert_eq!(value["payload"]["expected_version"], 7);
    }

    #[test]
    fn tool_approval_serialization_binds_run_and_invocation() {
        let command = LocalAgentCommand::DecideToolApproval(ToolApprovalCommand {
            run_id: "run-1".to_string(),
            invocation_id: "invocation-1".to_string(),
            decision: ToolApprovalDecision::Approve,
            reason: None,
        });
        command.validate().unwrap();
        let value = serde_json::to_value(command).unwrap();
        assert_eq!(value["type"], "decide_tool_approval");
        assert_eq!(value["payload"]["run_id"], "run-1");
        assert_eq!(value["payload"]["invocation_id"], "invocation-1");
    }

    #[test]
    fn rejects_an_unknown_protocol_version() {
        let request = LocalAgentIpcRequest {
            protocol_version: LOCAL_AGENT_PROTOCOL_VERSION + 1,
            request_id: "request-1".to_string(),
            owner_user_id: "user-1".to_string(),
            command: LocalAgentCommand::GetStorageProfile,
        };
        assert!(matches!(
            request.validate(),
            Err(ProtocolError::InvalidState { .. })
        ));
    }

    #[test]
    fn raw_database_credentials_have_no_wire_fields() {
        let command = LocalAgentCommand::TestPostgresConnection(PostgresConnectionTestCommand {
            connection_secret_reference: "keychain:postgres-1".to_string(),
        });
        let encoded = serde_json::to_string(&command).unwrap();
        assert!(encoded.contains("connection_secret_reference"));
        assert!(!encoded.contains("password"));
        assert!(!encoded.contains("database_url"));
    }
}
