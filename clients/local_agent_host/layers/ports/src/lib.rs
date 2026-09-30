// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

//! Persistence interfaces owned by the Local Agent application layer.

use async_trait::async_trait;
use chatos_local_agent_protocol::{
    CancelConversationTurnCommand, CreateConversationCommand, CreateTaskGraphCommand,
    GuideConversationTurnCommand, LocalAgentEventRecord, LocalAgentRunClaim,
    LocalAgentRunListScope, LocalAgentRunPage, LocalAgentRunRecord, LocalAgentRunStatus,
    LocalAgentToolApprovalDecision, LocalAgentToolApprovalResult, LocalAgentToolBatch,
    LocalAgentToolClaim, LocalAgentToolCommitResult, LocalAgentToolInvocationRecord,
    LocalAgentToolOutcome, LocalConversationDetail, LocalConversationHistoryPage,
    LocalConversationPage, LocalConversationRuntimeSettings, LocalConversationTurnStart,
    LocalConversationTurnUpdate, LocalNotepadImage, LocalNotepadNote, LocalNotepadNoteDetail,
    LocalPluginInstallationPage, LocalPluginInstallationRecord, LocalPluginInstallationSpec,
    LocalTaskGraph, LocalTaskGraphListScope, LocalTaskGraphPage,
    PutConversationRuntimeSettingsCommand, ResumeConversationTurnCommand,
    StartConversationTurnCommand, UpdateNotepadNoteCommand,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

mod memory_cache;
mod memory_outbox;

pub use memory_cache::LocalMemoryContextCacheStore;
pub use memory_outbox::{LocalMemoryOutboxRecord, LocalMemoryOutboxStatus, LocalMemoryOutboxStore};

pub use chatos_local_agent_protocol::{
    LocalCapabilityPolicySnapshot, LocalJsonSchemaOutputFormat, LocalMemorySyncStatus,
    LocalModelConfigSnapshot, MAX_CAPABILITY_INSTRUCTIONS_BYTES, MAX_CAPABILITY_ITEMS,
    MAX_CONTROL_PLANE_SNAPSHOT_BYTES,
};

#[derive(Debug, Error)]
pub enum ClientStorageError {
    #[error("client storage database error: {0}")]
    Database(String),
    #[error("client storage serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("record not found: {0}")]
    NotFound(String),
    #[error("storage conflict: {0}")]
    Conflict(String),
    #[error("invalid stored state: {0}")]
    InvalidState(String),
    #[error("command id was reused with different input: {0}")]
    CommandMismatch(String),
}

impl ClientStorageError {
    /// Erases an infrastructure-specific database error at the storage port
    /// boundary. Application packages must not depend on SQLx or SQLite types.
    pub fn database(error: impl std::fmt::Display) -> Self {
        Self::Database(error.to_string())
    }

    pub fn code(&self) -> &'static str {
        match self {
            Self::Database(_) => "storage_unavailable",
            Self::Serialization(_) | Self::InvalidState(_) => "storage_corrupt",
            Self::NotFound(_) => "not_found",
            Self::Conflict(_) => "conflict",
            Self::CommandMismatch(_) => "command_mismatch",
        }
    }

    pub fn retryable(&self) -> bool {
        matches!(self, Self::Database(_))
    }
}

#[derive(Debug, Clone)]
pub struct IdempotentCommand {
    pub command_id: String,
    pub request_fingerprint: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RunTransition {
    pub run_id: String,
    pub claim_token: String,
    pub expected_version: u64,
    pub expected_status: LocalAgentRunStatus,
    pub next_status: LocalAgentRunStatus,
    pub next_model_attempt: u32,
    pub next_attempt_at_unix_ms: Option<i64>,
    pub pending_tool_batch: Option<Value>,
    pub tool_batch: Option<LocalAgentToolBatch>,
    pub checkpoint: Option<Value>,
    pub clear_continuation_input: bool,
    pub terminal_outcome: Option<Value>,
    pub event_id: String,
    pub event_type: String,
    pub event_payload: Value,
    pub occurred_at_unix_ms: i64,
}

#[async_trait]
pub trait LocalAgentRunStore: Send + Sync {
    async fn create_run(
        &self,
        command: &IdempotentCommand,
        run: &LocalAgentRunRecord,
        event_id: &str,
    ) -> Result<LocalAgentRunRecord, ClientStorageError>;

    async fn get_run(
        &self,
        run_id: &str,
    ) -> Result<Option<LocalAgentRunRecord>, ClientStorageError>;

    async fn get_run_for_owner(
        &self,
        owner_user_id: &str,
        run_id: &str,
    ) -> Result<Option<LocalAgentRunRecord>, ClientStorageError>;

    async fn list_runs(
        &self,
        owner_user_id: &str,
        scope: LocalAgentRunListScope,
        before_updated_at_unix_ms: Option<i64>,
        before_run_id: Option<&str>,
        limit: u32,
    ) -> Result<LocalAgentRunPage, ClientStorageError>;

    async fn recover_expired_claims(
        &self,
        owner_user_id: &str,
        now_unix_ms: i64,
    ) -> Result<u64, ClientStorageError>;

    async fn claim_next_run(
        &self,
        command: &IdempotentCommand,
        owner_user_id: &str,
        worker_id: &str,
        claim_token: &str,
        now_unix_ms: i64,
        claim_until_unix_ms: i64,
        event_id: &str,
    ) -> Result<Option<LocalAgentRunClaim>, ClientStorageError>;

    async fn next_retry_at(&self, owner_user_id: &str) -> Result<Option<i64>, ClientStorageError>;

    async fn apply_transition(
        &self,
        command: &IdempotentCommand,
        transition: &RunTransition,
    ) -> Result<LocalAgentRunRecord, ClientStorageError>;

    #[allow(clippy::too_many_arguments)]
    async fn resume_run(
        &self,
        command: &IdempotentCommand,
        run_id: &str,
        expected_version: u64,
        expected_status: LocalAgentRunStatus,
        continuation_input: &Value,
        event_id: &str,
        now_unix_ms: i64,
    ) -> Result<LocalAgentRunRecord, ClientStorageError>;

    #[allow(clippy::too_many_arguments)]
    async fn resume_run_for_owner(
        &self,
        command: &IdempotentCommand,
        owner_user_id: &str,
        run_id: &str,
        expected_version: u64,
        expected_status: LocalAgentRunStatus,
        continuation_input: &Value,
        event_id: &str,
        now_unix_ms: i64,
    ) -> Result<LocalAgentRunRecord, ClientStorageError>;

    async fn cancel_run(
        &self,
        command: &IdempotentCommand,
        run_id: &str,
        expected_version: Option<u64>,
        reason: &str,
        event_id: &str,
        now_unix_ms: i64,
    ) -> Result<LocalAgentRunRecord, ClientStorageError>;

    #[allow(clippy::too_many_arguments)]
    async fn cancel_run_for_owner(
        &self,
        command: &IdempotentCommand,
        owner_user_id: &str,
        run_id: &str,
        expected_version: Option<u64>,
        reason: &str,
        event_id: &str,
        now_unix_ms: i64,
    ) -> Result<LocalAgentRunRecord, ClientStorageError>;

    async fn list_events(
        &self,
        after_cursor: i64,
        limit: u32,
        run_id: Option<&str>,
    ) -> Result<Vec<LocalAgentEventRecord>, ClientStorageError>;

    async fn list_events_for_owner(
        &self,
        owner_user_id: &str,
        after_cursor: i64,
        limit: u32,
        run_id: Option<&str>,
    ) -> Result<Vec<LocalAgentEventRecord>, ClientStorageError>;

    async fn health_check(&self) -> Result<(), ClientStorageError>;
}

#[async_trait]
pub trait LocalAgentTaskStore: Send + Sync {
    async fn create_task_graph(
        &self,
        command: &IdempotentCommand,
        graph: &CreateTaskGraphCommand,
        now_unix_ms: i64,
    ) -> Result<LocalTaskGraph, ClientStorageError>;

    async fn get_task_graph(
        &self,
        owner_user_id: &str,
        graph_id: &str,
    ) -> Result<Option<LocalTaskGraph>, ClientStorageError>;

    async fn list_task_graphs(
        &self,
        owner_user_id: &str,
        scope: LocalTaskGraphListScope,
        before_updated_at_unix_ms: Option<i64>,
        before_graph_id: Option<&str>,
        limit: u32,
    ) -> Result<LocalTaskGraphPage, ClientStorageError>;

    async fn list_task_runs(
        &self,
        owner_user_id: &str,
        task_id: &str,
        limit: u32,
    ) -> Result<Vec<LocalAgentRunRecord>, ClientStorageError>;

    async fn start_next_task_run(
        &self,
        owner_user_id: &str,
        run_id: &str,
        event_id: &str,
        now_unix_ms: i64,
    ) -> Result<Option<LocalAgentRunRecord>, ClientStorageError>;

    #[allow(clippy::too_many_arguments)]
    async fn cancel_task(
        &self,
        command: &IdempotentCommand,
        owner_user_id: &str,
        task_id: &str,
        expected_version: Option<u64>,
        reason: &str,
        run_event_id: &str,
        now_unix_ms: i64,
    ) -> Result<LocalTaskGraph, ClientStorageError>;

    async fn retry_task(
        &self,
        command: &IdempotentCommand,
        owner_user_id: &str,
        task_id: &str,
        expected_version: u64,
        retry_instruction: Option<&str>,
        now_unix_ms: i64,
    ) -> Result<LocalTaskGraph, ClientStorageError>;

    #[allow(clippy::too_many_arguments)]
    async fn restart_task(
        &self,
        command: &IdempotentCommand,
        owner_user_id: &str,
        task_id: &str,
        expected_version: u64,
        reason: &str,
        run_event_prefix: &str,
        now_unix_ms: i64,
    ) -> Result<LocalTaskGraph, ClientStorageError>;
}

#[async_trait]
pub trait LocalAgentToolStore: Send + Sync {
    async fn recover_expired_tool_claims(
        &self,
        owner_user_id: &str,
        now_unix_ms: i64,
    ) -> Result<u64, ClientStorageError>;

    #[allow(clippy::too_many_arguments)]
    async fn claim_next_tool(
        &self,
        command: &IdempotentCommand,
        owner_user_id: &str,
        worker_id: &str,
        claim_token: &str,
        now_unix_ms: i64,
        claim_until_unix_ms: i64,
        event_id: &str,
        include_tool_names: Option<&[String]>,
        exclude_tool_names: &[String],
    ) -> Result<Option<LocalAgentToolClaim>, ClientStorageError>;

    #[allow(clippy::too_many_arguments)]
    async fn commit_tool(
        &self,
        command: &IdempotentCommand,
        owner_user_id: &str,
        invocation_id: &str,
        claim_token: &str,
        expected_version: u64,
        outcome: &LocalAgentToolOutcome,
        event_id: &str,
        batch_event_id: &str,
        now_unix_ms: i64,
    ) -> Result<LocalAgentToolCommitResult, ClientStorageError>;

    async fn list_pending_tool_approvals(
        &self,
        owner_user_id: &str,
        limit: u32,
    ) -> Result<Vec<LocalAgentToolInvocationRecord>, ClientStorageError>;

    #[allow(clippy::too_many_arguments)]
    async fn decide_tool_approval(
        &self,
        command: &IdempotentCommand,
        owner_user_id: &str,
        invocation_id: &str,
        expected_version: u64,
        decision: LocalAgentToolApprovalDecision,
        decided_by: &str,
        reason: &str,
        event_id: &str,
        batch_event_id: &str,
        now_unix_ms: i64,
    ) -> Result<LocalAgentToolApprovalResult, ClientStorageError>;
}

#[async_trait]
pub trait LocalPluginInstallationStore: Send + Sync {
    async fn put_plugin_installation(
        &self,
        command: &IdempotentCommand,
        installation: &LocalPluginInstallationSpec,
        expected_version: Option<u64>,
        now_unix_ms: i64,
    ) -> Result<LocalPluginInstallationRecord, ClientStorageError>;

    async fn get_plugin_installation(
        &self,
        owner_user_id: &str,
        installation_id: &str,
    ) -> Result<Option<LocalPluginInstallationRecord>, ClientStorageError>;

    async fn list_plugin_installations(
        &self,
        owner_user_id: &str,
        before_updated_at_unix_ms: Option<i64>,
        before_installation_id: Option<&str>,
        limit: u32,
    ) -> Result<LocalPluginInstallationPage, ClientStorageError>;

    async fn remove_plugin_installation(
        &self,
        command: &IdempotentCommand,
        owner_user_id: &str,
        installation_id: &str,
        expected_version: u64,
        now_unix_ms: i64,
    ) -> Result<LocalPluginInstallationRecord, ClientStorageError>;
}

#[async_trait]
pub trait LocalCapabilitySnapshotStore: Send + Sync {
    /// Stores an immutable revision. Repeating the same value is safe; reusing
    /// its key for different content must be rejected as a conflict.
    async fn put_capability_snapshot(
        &self,
        command: &IdempotentCommand,
        snapshot: &LocalCapabilityPolicySnapshot,
        now_unix_ms: i64,
    ) -> Result<LocalCapabilityPolicySnapshot, ClientStorageError>;

    async fn get_capability_snapshot(
        &self,
        owner_user_id: &str,
        profile_key: &str,
        capability_policy_revision: &str,
    ) -> Result<Option<LocalCapabilityPolicySnapshot>, ClientStorageError>;
}

#[async_trait]
pub trait LocalModelConfigSnapshotStore: Send + Sync {
    /// Stores a non-secret immutable model configuration revision. API keys
    /// are represented only by native credential-store references.
    async fn put_model_config_snapshot(
        &self,
        command: &IdempotentCommand,
        snapshot: &LocalModelConfigSnapshot,
        now_unix_ms: i64,
    ) -> Result<LocalModelConfigSnapshot, ClientStorageError>;

    async fn get_model_config_snapshot(
        &self,
        owner_user_id: &str,
        model_config_ref: &str,
        model_config_revision: &str,
    ) -> Result<Option<LocalModelConfigSnapshot>, ClientStorageError>;
}

#[async_trait]
pub trait LocalConversationStore: Send + Sync {
    async fn create_conversation(
        &self,
        command: &IdempotentCommand,
        conversation: &CreateConversationCommand,
        now_unix_ms: i64,
    ) -> Result<LocalConversationDetail, ClientStorageError>;

    async fn get_conversation(
        &self,
        owner_user_id: &str,
        conversation_id: &str,
    ) -> Result<Option<LocalConversationDetail>, ClientStorageError>;

    async fn get_conversation_history(
        &self,
        owner_user_id: &str,
        conversation_id: &str,
        before_ordinal: Option<u64>,
        limit: u32,
    ) -> Result<LocalConversationHistoryPage, ClientStorageError>;

    async fn list_conversations(
        &self,
        owner_user_id: &str,
        before_updated_at_unix_ms: Option<i64>,
        before_conversation_id: Option<&str>,
        limit: u32,
    ) -> Result<LocalConversationPage, ClientStorageError>;

    async fn start_conversation_turn(
        &self,
        command: &IdempotentCommand,
        turn: &StartConversationTurnCommand,
        run: &LocalAgentRunRecord,
        event_id: &str,
        now_unix_ms: i64,
    ) -> Result<LocalConversationTurnStart, ClientStorageError>;

    async fn guide_conversation_turn(
        &self,
        command: &IdempotentCommand,
        turn: &GuideConversationTurnCommand,
        event_id: &str,
        now_unix_ms: i64,
    ) -> Result<LocalConversationTurnUpdate, ClientStorageError>;

    #[allow(clippy::too_many_arguments)]
    async fn resume_conversation_turn(
        &self,
        command: &IdempotentCommand,
        turn: &ResumeConversationTurnCommand,
        continuation_input: &Value,
        event_id: &str,
        now_unix_ms: i64,
    ) -> Result<LocalConversationTurnUpdate, ClientStorageError>;

    async fn cancel_conversation_turn(
        &self,
        command: &IdempotentCommand,
        turn: &CancelConversationTurnCommand,
        event_id: &str,
        now_unix_ms: i64,
    ) -> Result<LocalConversationTurnUpdate, ClientStorageError>;
}

#[async_trait]
pub trait LocalConversationRuntimeSettingsStore: Send + Sync {
    async fn get_conversation_runtime_settings(
        &self,
        owner_user_id: &str,
        conversation_id: &str,
    ) -> Result<Option<LocalConversationRuntimeSettings>, ClientStorageError>;

    async fn put_conversation_runtime_settings(
        &self,
        command: &IdempotentCommand,
        settings: &PutConversationRuntimeSettingsCommand,
        now_unix_ms: i64,
    ) -> Result<LocalConversationRuntimeSettings, ClientStorageError>;
}

#[derive(Debug, Clone)]
pub struct LocalNotepadImageWrite {
    pub image: LocalNotepadImage,
    pub data: Vec<u8>,
}

#[async_trait]
pub trait LocalNotepadStore: Send + Sync {
    async fn initialize_notepad(&self, owner_user_id: &str) -> Result<u64, ClientStorageError>;

    async fn list_notepad_folders(
        &self,
        owner_user_id: &str,
    ) -> Result<Vec<String>, ClientStorageError>;

    async fn create_notepad_folder(
        &self,
        command: &IdempotentCommand,
        owner_user_id: &str,
        folder: &str,
        now_unix_ms: i64,
    ) -> Result<String, ClientStorageError>;

    async fn rename_notepad_folder(
        &self,
        command: &IdempotentCommand,
        owner_user_id: &str,
        from: &str,
        to: &str,
        now_unix_ms: i64,
    ) -> Result<u64, ClientStorageError>;

    async fn delete_notepad_folder(
        &self,
        command: &IdempotentCommand,
        owner_user_id: &str,
        folder: &str,
        recursive: bool,
        now_unix_ms: i64,
    ) -> Result<u64, ClientStorageError>;

    async fn list_notepad_notes(
        &self,
        owner_user_id: &str,
        query: Option<&str>,
        limit: u32,
    ) -> Result<Vec<LocalNotepadNote>, ClientStorageError>;

    async fn create_notepad_note(
        &self,
        command: &IdempotentCommand,
        note_id: &str,
        owner_user_id: &str,
        folder: &str,
        title: &str,
        content: &str,
        tags: &[String],
        now_unix_ms: i64,
    ) -> Result<LocalNotepadNoteDetail, ClientStorageError>;

    async fn get_notepad_note(
        &self,
        owner_user_id: &str,
        note_id: &str,
    ) -> Result<Option<LocalNotepadNoteDetail>, ClientStorageError>;

    async fn update_notepad_note(
        &self,
        command: &IdempotentCommand,
        update: &UpdateNotepadNoteCommand,
        now_unix_ms: i64,
    ) -> Result<LocalNotepadNoteDetail, ClientStorageError>;

    async fn delete_notepad_note(
        &self,
        command: &IdempotentCommand,
        owner_user_id: &str,
        note_id: &str,
        expected_version: u64,
        now_unix_ms: i64,
    ) -> Result<(), ClientStorageError>;

    async fn put_notepad_image(
        &self,
        command: &IdempotentCommand,
        write: &LocalNotepadImageWrite,
    ) -> Result<LocalNotepadImage, ClientStorageError>;
}

pub trait LocalAgentStore:
    LocalAgentRunStore
    + LocalAgentToolStore
    + LocalAgentTaskStore
    + LocalPluginInstallationStore
    + LocalConversationStore
    + LocalConversationRuntimeSettingsStore
    + LocalNotepadStore
    + LocalCapabilitySnapshotStore
    + LocalModelConfigSnapshotStore
    + LocalMemoryOutboxStore
    + LocalMemoryContextCacheStore
{
}

impl<T> LocalAgentStore for T where
    T: LocalAgentRunStore
        + LocalAgentToolStore
        + LocalAgentTaskStore
        + LocalPluginInstallationStore
        + LocalConversationStore
        + LocalConversationRuntimeSettingsStore
        + LocalNotepadStore
        + LocalCapabilitySnapshotStore
        + LocalModelConfigSnapshotStore
        + LocalMemoryOutboxStore
        + LocalMemoryContextCacheStore
{
}
