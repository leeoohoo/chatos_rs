// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chrono::{DateTime, Utc};
use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::{
    require_bounded_json, require_digest, require_identifier, LocalAgentRun, ProtocolError,
    ToolExecution, LOCAL_AGENT_PROTOCOL_VERSION,
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
    CreateMainChatTurn(CreateMainChatTurnCommand),
    CreateTask(Box<CreateTaskCommand>),
    PauseRun { run_id: String },
    ResumeRun { run_id: String },
    CancelRun { run_id: String },
    AnswerUserQuestion(AnswerUserQuestionCommand),
    DecideToolApproval(ToolApprovalCommand),
    GetRun { run_id: String },
    ListRuns { cursor: Option<String>, limit: u32 },
    SubscribeRunEvents { after_seq: u64, limit: u32 },
    GetStorageProfile,
    TestPostgresConnection(PostgresConnectionTestCommand),
    ApplyStorageProfile(ApplyStorageProfileCommand),
    ExportClientData(ExportClientDataCommand),
    ImportClientData(ImportClientDataCommand),
}

impl LocalAgentCommand {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        match self {
            Self::CreateMainChatTurn(command) => command.validate(),
            Self::CreateTask(command) => command.validate(),
            Self::PauseRun { run_id }
            | Self::ResumeRun { run_id }
            | Self::CancelRun { run_id }
            | Self::GetRun { run_id } => require_identifier("run_id", run_id),
            Self::AnswerUserQuestion(command) => command.validate(),
            Self::DecideToolApproval(command) => command.validate(),
            Self::ListRuns { cursor, limit } => validate_page(cursor.as_deref(), *limit),
            Self::SubscribeRunEvents { limit, .. } => validate_page(None, *limit),
            Self::GetStorageProfile => Ok(()),
            Self::TestPostgresConnection(command) => command.validate(),
            Self::ApplyStorageProfile(command) => command.validate(),
            Self::ExportClientData(command) => command.validate(),
            Self::ImportClientData(command) => command.validate(),
        }
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
    pub prompt_revision: String,
    pub capability_snapshot_ref: String,
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
            ("prompt_revision", self.prompt_revision.as_str()),
            (
                "capability_snapshot_ref",
                self.capability_snapshot_ref.as_str(),
            ),
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
        for attachment in &self.attachments {
            attachment.validate()?;
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
    fn validate(&self) -> Result<(), ProtocolError> {
        require_identifier("attachment_id", &self.attachment_id)?;
        require_identifier("media_type", &self.media_type)?;
        require_identifier("payload_reference", &self.payload_reference)?;
        require_digest("payload_digest", &self.payload_digest)?;
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
    pub invocation_id: String,
    pub decision: ToolApprovalDecision,
    pub reason: Option<String>,
}

impl ToolApprovalCommand {
    fn validate(&self) -> Result<(), ProtocolError> {
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

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ClientStorageBackendKind {
    Sqlite,
    Postgres,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ClientStorageHealth {
    Active,
    RestartRequired,
    Unavailable,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ClientStorageProfileDescriptor {
    pub backend: ClientStorageBackendKind,
    pub health: ClientStorageHealth,
    pub sqlite_database_reference: Option<String>,
    pub postgres_connection_secret_reference: Option<String>,
    pub schema_version: u32,
    pub last_error_code: Option<String>,
}

impl ClientStorageProfileDescriptor {
    fn validate(&self) -> Result<(), ProtocolError> {
        if self.schema_version == 0 {
            return Err(ProtocolError::InvalidState {
                reason: "storage schema version must be positive",
            });
        }
        match self.backend {
            ClientStorageBackendKind::Sqlite => {
                require_identifier(
                    "sqlite_database_reference",
                    self.sqlite_database_reference
                        .as_deref()
                        .unwrap_or_default(),
                )?;
                if self.postgres_connection_secret_reference.is_some() {
                    return Err(ProtocolError::InvalidState {
                        reason: "SQLite profile cannot include a PostgreSQL secret reference",
                    });
                }
            }
            ClientStorageBackendKind::Postgres => {
                require_identifier(
                    "postgres_connection_secret_reference",
                    self.postgres_connection_secret_reference
                        .as_deref()
                        .unwrap_or_default(),
                )?;
                if self.sqlite_database_reference.is_some() {
                    return Err(ProtocolError::InvalidState {
                        reason: "PostgreSQL profile cannot include a SQLite database reference",
                    });
                }
            }
        }
        if let Some(code) = &self.last_error_code {
            require_identifier("storage_error_code", code)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "backend", rename_all = "snake_case")]
pub enum ClientStorageProfileSelection {
    Sqlite {
        database_reference: String,
        encryption_secret_reference: String,
    },
    Postgres {
        connection_secret_reference: String,
    },
}

impl ClientStorageProfileSelection {
    fn validate(&self) -> Result<(), ProtocolError> {
        match self {
            Self::Sqlite {
                database_reference,
                encryption_secret_reference,
            } => {
                require_identifier("database_reference", database_reference)?;
                require_identifier("encryption_secret_reference", encryption_secret_reference)
            }
            Self::Postgres {
                connection_secret_reference,
            } => require_identifier("connection_secret_reference", connection_secret_reference),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PostgresConnectionTestCommand {
    pub connection_secret_reference: String,
}

impl PostgresConnectionTestCommand {
    fn validate(&self) -> Result<(), ProtocolError> {
        require_identifier(
            "connection_secret_reference",
            &self.connection_secret_reference,
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PostgresConnectionTestResult {
    pub server_version: String,
    pub tls_active: bool,
    pub authentication_ok: bool,
    pub transaction_ok: bool,
    pub migration_permission_ok: bool,
}

impl PostgresConnectionTestResult {
    fn validate(&self) -> Result<(), ProtocolError> {
        require_identifier("postgres_server_version", &self.server_version)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ApplyStorageProfileCommand {
    pub profile: ClientStorageProfileSelection,
    pub confirm_no_active_runs: bool,
}

impl ApplyStorageProfileCommand {
    fn validate(&self) -> Result<(), ProtocolError> {
        if !self.confirm_no_active_runs {
            return Err(ProtocolError::InvalidState {
                reason: "applying a storage profile requires explicit active Run confirmation",
            });
        }
        self.profile.validate()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ExportClientDataCommand {
    pub destination_reference: String,
    pub include_large_payload_references: bool,
}

impl ExportClientDataCommand {
    fn validate(&self) -> Result<(), ProtocolError> {
        require_identifier("export_destination_reference", &self.destination_reference)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ImportClientDataCommand {
    pub source_reference: String,
    pub expected_archive_digest: String,
    pub confirm_no_active_runs: bool,
}

impl ImportClientDataCommand {
    fn validate(&self) -> Result<(), ProtocolError> {
        require_identifier("import_source_reference", &self.source_reference)?;
        require_digest("expected_archive_digest", &self.expected_archive_digest)?;
        if !self.confirm_no_active_runs {
            return Err(ProtocolError::InvalidState {
                reason: "import requires explicit active Run confirmation",
            });
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ClientDataTransferResult {
    pub archive_reference: String,
    pub archive_digest: String,
    pub record_count: u64,
}

impl ClientDataTransferResult {
    fn validate(&self) -> Result<(), ProtocolError> {
        require_identifier("archive_reference", &self.archive_reference)?;
        require_digest("archive_digest", &self.archive_digest)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", content = "payload", rename_all = "snake_case")]
pub enum LocalAgentIpcResponse {
    Accepted {
        operation_id: String,
    },
    Run(Box<LocalAgentRun>),
    Runs {
        runs: Vec<LocalAgentRun>,
        next_cursor: Option<String>,
    },
    Events {
        events: Vec<LocalAgentUiEvent>,
        next_seq: u64,
        has_more: bool,
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
            Self::Run(run) => run.validate(),
            Self::Runs { runs, next_cursor } => {
                for run in runs {
                    run.validate()?;
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
    pub run_id: Option<String>,
    pub pending_count: u64,
    pub failed_count: u64,
    pub last_error_code: Option<String>,
}

impl MemorySyncUiStatus {
    fn validate(&self) -> Result<(), ProtocolError> {
        if let Some(run_id) = &self.run_id {
            require_identifier("run_id", run_id)?;
        }
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
        let value = serde_json::to_value(LocalAgentCommand::PauseRun {
            run_id: "run-1".to_string(),
        })
        .unwrap();
        assert_eq!(value["type"], "pause_run");
        assert_eq!(value["payload"]["run_id"], "run-1");
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
