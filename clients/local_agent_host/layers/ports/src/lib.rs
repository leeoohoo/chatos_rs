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
    LocalConversationTurnUpdate, LocalNotepadImage, LocalNotepadNote, LocalNotepadNoteDetail,
    LocalPluginHookBundleRecord, LocalPluginHookBundleSpec, LocalPluginInstallationPage,
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
    #[error("storage maintenance required: {0}")]
    MaintenanceRequired(String),
    #[error(
        "command completed but its original response is no longer retained; reconcile the \
             original operation instead of resubmitting it with a new command id: {0}"
    )]
    ReplayExpired(String),
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
            Self::MaintenanceRequired(_) => "storage_maintenance_required",
            Self::ReplayExpired(_) => "command_replay_expired",
        }
    }

    pub fn retryable(&self) -> bool {
        matches!(self, Self::Database(_))
    }
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
        status: Option<LocalAgentRunStatus>,
        updated_after_unix_ms: Option<i64>,
        before_updated_at_unix_ms: Option<i64>,
        before_run_id: Option<&str>,
        limit: u32,
    ) -> Result<LocalAgentRunPage, ClientStorageError>;

    async fn recover_expired_claims(
        &self,
        owner_user_id: &str,
        now_unix_ms: i64,
    ) -> Result<u64, ClientStorageError>;

    #[allow(clippy::too_many_arguments)]
    async fn renew_run_claim(
        &self,
        owner_user_id: &str,
        run_id: &str,
        claim_token: &str,
        expected_version: u64,
        now_unix_ms: i64,
        claim_until_unix_ms: i64,
    ) -> Result<bool, ClientStorageError>;

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

    /// Returns the earliest retry or active claim-expiry deadline that must
    /// wake the owner-scoped coordinator.
    async fn next_retry_at(&self, owner_user_id: &str) -> Result<Option<i64>, ClientStorageError>;

    /// Counts durable model, task, and tool work that is ready for a Host worker to claim.
    /// The tool filters must match the built-in tool scheduler so platform-owned work is not
    /// reported as queued behind Host capacity.
    async fn ready_host_work_count(
        &self,
        owner_user_id: &str,
        now_unix_ms: i64,
        include_tool_names: Option<&[String]>,
        exclude_tool_names: &[String],
        tool_execution_route: Option<chatos_local_agent_protocol::LocalAgentToolExecutionRoute>,
    ) -> Result<u64, ClientStorageError>;

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
        event_type: Option<&str>,
        newest_first: bool,
        payload_mode: LocalAgentEventPayloadMode,
    ) -> Result<Vec<LocalAgentEventRecord>, ClientStorageError>;

    async fn latest_event_cursor_for_owner(
        &self,
        owner_user_id: &str,
    ) -> Result<i64, ClientStorageError>;

    async fn health_check(&self) -> Result<(), ClientStorageError>;
}

mod task_store_port;
pub use task_store_port::LocalAgentTaskStore;

#[async_trait]
pub trait LocalAgentToolStore: Send + Sync {
    async fn get_tool_invocation(
        &self,
        invocation_id: &str,
    ) -> Result<Option<LocalAgentToolInvocationRecord>, ClientStorageError>;

    /// Resolves one completed source call inside the active owner's Run.
    /// This is the only lookup used by progressive tool-result reads; callers
    /// cannot cross owner or Run boundaries with a model-supplied call id.
    async fn get_completed_tool_invocation_for_run_call(
        &self,
        owner_user_id: &str,
        run_id: &str,
        call_id: &str,
    ) -> Result<Option<LocalAgentToolInvocationRecord>, ClientStorageError>;

    async fn successful_tool_invocation_count(
        &self,
        run_id: &str,
        tool_name: &str,
    ) -> Result<u64, ClientStorageError>;

    async fn has_successful_tool_invocation_argument(
        &self,
        run_id: &str,
        tool_name: &str,
        argument_name: &str,
        argument_value: &str,
    ) -> Result<bool, ClientStorageError>;

    async fn successful_tool_invocation_argument_values(
        &self,
        owner_user_id: &str,
        run_id: &str,
        tool_name: &str,
        argument_name: &str,
    ) -> Result<Vec<String>, ClientStorageError>;

    /// Returns successful, non-error calls for one tool inside an owner-scoped Run.
    /// Used to restore durable control-plane facts after a native provider restart.
    async fn successful_tool_invocations_for_run(
        &self,
        owner_user_id: &str,
        run_id: &str,
        tool_name: &str,
    ) -> Result<Vec<LocalAgentToolInvocationRecord>, ClientStorageError>;

    /// Trusted, owner/Run-scoped facts only; never model-provided descriptions.
    #[allow(clippy::too_many_arguments)]
    async fn latest_successful_tool_invocation_result(
        &self,
        owner_user_id: &str,
        run_id: &str,
        tool_name: &str,
        argument_name: &str,
        argument_value: &str,
        described_tool_option: Option<&str>,
    ) -> Result<Option<Value>, ClientStorageError>;

    async fn recover_expired_tool_claims(
        &self,
        owner_user_id: &str,
        now_unix_ms: i64,
    ) -> Result<u64, ClientStorageError>;

    #[allow(clippy::too_many_arguments)]
    async fn renew_tool_claim(
        &self,
        owner_user_id: &str,
        invocation_id: &str,
        claim_token: &str,
        expected_version: u64,
        now_unix_ms: i64,
        claim_until_unix_ms: i64,
    ) -> Result<bool, ClientStorageError>;

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
        execution_route: Option<chatos_local_agent_protocol::LocalAgentToolExecutionRoute>,
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
pub trait LocalPluginHookBundleStore: Send + Sync {
    /// Appends an immutable trust snapshot. Repeating the same bundle is
    /// idempotent; reusing its identity for different material is a conflict.
    async fn put_plugin_hook_bundle(
        &self,
        command: &IdempotentCommand,
        bundle: &LocalPluginHookBundleSpec,
        now_unix_ms: i64,
    ) -> Result<LocalPluginHookBundleRecord, ClientStorageError>;

    async fn list_plugin_hook_bundles(
        &self,
        owner_user_id: &str,
    ) -> Result<Vec<LocalPluginHookBundleRecord>, ClientStorageError>;

    /// Loads immutable trusted bundles by their exact event-time identities,
    /// regardless of their current activation state.
    async fn get_plugin_hook_bundles(
        &self,
        owner_user_id: &str,
        bundle_ids: &[String],
    ) -> Result<Vec<LocalPluginHookBundleRecord>, ClientStorageError>;

    async fn set_active_plugin_hook_bundles(
        &self,
        command: &IdempotentCommand,
        owner_user_id: &str,
        bundle_ids: &[String],
        now_unix_ms: i64,
    ) -> Result<Vec<String>, ClientStorageError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalHookEventSpec {
    pub event_id: String,
    pub owner_user_id: String,
    pub event_type: String,
    pub subject_id: String,
    pub sequence: u64,
    pub payload_sha256: String,
    pub payload_json: String,
    pub matcher_value: String,
    pub active_bundle_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalHookEventRecord {
    pub spec: LocalHookEventSpec,
    pub status: String,
    pub additional_context: Option<String>,
    pub created_at_unix_ms: i64,
    pub updated_at_unix_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalHookInvocationSpec {
    pub invocation_id: String,
    pub event_id: String,
    pub bundle_id: String,
    pub hook_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalHookInvocationRecord {
    pub spec: LocalHookInvocationSpec,
    pub attempt: u32,
    pub status: String,
    pub exit_code: Option<i32>,
    pub timed_out: bool,
    pub duration_ms: u64,
    pub stdout_sha256: Option<String>,
    pub stdout_bytes: u64,
    pub stdout_overflow: bool,
    pub stderr_sha256: Option<String>,
    pub stderr_bytes: u64,
    pub stderr_overflow: bool,
    pub stderr_summary: Option<String>,
    pub additional_context: Option<String>,
    pub created_at_unix_ms: i64,
    pub updated_at_unix_ms: i64,
}

#[derive(Clone, PartialEq, Eq)]
pub struct LocalHookOutputSpill {
    bytes: Vec<u8>,
}

impl LocalHookOutputSpill {
    pub fn new(bytes: Vec<u8>) -> Self {
        Self { bytes }
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }

    pub fn byte_count(&self) -> usize {
        self.bytes.len()
    }
}

impl std::fmt::Debug for LocalHookOutputSpill {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("LocalHookOutputSpill")
            .field("byte_count", &self.bytes.len())
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalHookOutputBlobRecord {
    pub blob_id: String,
    pub invocation_id: String,
    pub stream: String,
    pub sha256: String,
    pub byte_count: u64,
    pub storage_ref: String,
    pub expires_at_unix_ms: i64,
    pub created_at_unix_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalHookInvocationCompletion {
    pub status: String,
    pub exit_code: Option<i32>,
    pub timed_out: bool,
    pub duration_ms: u64,
    pub stdout_sha256: Option<String>,
    pub stdout_bytes: u64,
    pub stdout_overflow: bool,
    pub stderr_sha256: Option<String>,
    pub stderr_bytes: u64,
    pub stderr_overflow: bool,
    pub stderr_summary: Option<String>,
    pub additional_context: Option<String>,
    pub stdout_spill: Option<LocalHookOutputSpill>,
    pub stderr_spill: Option<LocalHookOutputSpill>,
}

#[async_trait]
pub trait LocalHookEventStore: Send + Sync {
    async fn has_hook_event(
        &self,
        owner_user_id: &str,
        event_type: &str,
        subject_id: &str,
    ) -> Result<bool, ClientStorageError>;

    async fn get_hook_event(
        &self,
        owner_user_id: &str,
        event_type: &str,
        subject_id: &str,
        sequence: u64,
    ) -> Result<Option<LocalHookEventRecord>, ClientStorageError>;

    async fn list_pending_hook_events(
        &self,
        owner_user_id: &str,
        limit: u32,
    ) -> Result<Vec<LocalHookEventRecord>, ClientStorageError>;

    async fn prepare_hook_event(
        &self,
        event: &LocalHookEventSpec,
        now_unix_ms: i64,
    ) -> Result<LocalHookEventRecord, ClientStorageError>;

    async fn begin_hook_invocation(
        &self,
        invocation: &LocalHookInvocationSpec,
        now_unix_ms: i64,
    ) -> Result<LocalHookInvocationRecord, ClientStorageError>;

    async fn complete_hook_invocation(
        &self,
        invocation_id: &str,
        expected_attempt: u32,
        completion: &LocalHookInvocationCompletion,
        now_unix_ms: i64,
    ) -> Result<LocalHookInvocationRecord, ClientStorageError>;

    async fn list_hook_output_blobs(
        &self,
        invocation_id: &str,
    ) -> Result<Vec<LocalHookOutputBlobRecord>, ClientStorageError>;

    async fn complete_hook_event(
        &self,
        event_id: &str,
        additional_context: Option<&str>,
        now_unix_ms: i64,
    ) -> Result<LocalHookEventRecord, ClientStorageError>;
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

    async fn get_latest_capability_snapshot(
        &self,
        owner_user_id: &str,
        profile_key: &str,
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

    /// Resolves the most recently published immutable revision for a model id.
    /// Task creation uses this to preserve the former Task Runner model override
    /// contract without guessing or copying credentials into a Task record.
    async fn get_latest_model_config_snapshot(
        &self,
        owner_user_id: &str,
        model_config_ref: &str,
    ) -> Result<Option<LocalModelConfigSnapshot>, ClientStorageError>;

    async fn list_latest_model_config_snapshots(
        &self,
        owner_user_id: &str,
    ) -> Result<Vec<LocalModelConfigSnapshot>, ClientStorageError>;
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

    async fn get_conversation_context(
        &self,
        owner_user_id: &str,
        conversation_id: &str,
    ) -> Result<Option<LocalConversationDetail>, ClientStorageError>;

    async fn get_conversation_attachment(
        &self,
        owner_user_id: &str,
        conversation_id: &str,
        authorized_local_ref: &str,
    ) -> Result<Option<LocalConversationAttachmentRecord>, ClientStorageError>;

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

#[async_trait]
pub trait LocalRemoteConnectionStore: Send + Sync {
    async fn list_remote_connections(
        &self,
        owner_user_id: &str,
    ) -> Result<Vec<LocalRemoteConnection>, ClientStorageError>;

    async fn get_remote_connection(
        &self,
        owner_user_id: &str,
        connection_id: &str,
    ) -> Result<Option<LocalRemoteConnection>, ClientStorageError>;

    async fn create_remote_connection(
        &self,
        command: &IdempotentCommand,
        connection: &LocalRemoteConnection,
    ) -> Result<LocalRemoteConnection, ClientStorageError>;

    async fn update_remote_connection(
        &self,
        command: &IdempotentCommand,
        connection: &LocalRemoteConnection,
        expected_version: u64,
    ) -> Result<LocalRemoteConnection, ClientStorageError>;

    async fn delete_remote_connection(
        &self,
        command: &IdempotentCommand,
        owner_user_id: &str,
        connection_id: &str,
        expected_version: u64,
        now_unix_ms: i64,
    ) -> Result<(), ClientStorageError>;
}

#[async_trait]
pub trait LocalAgentArtifactStore: Send + Sync {
    async fn create_artifact(
        &self,
        command: &IdempotentCommand,
        write: &LocalAgentArtifactWrite,
    ) -> Result<LocalAgentArtifact, ClientStorageError>;

    async fn list_artifacts(
        &self,
        owner_user_id: &str,
        before_updated_at_unix_ms: Option<i64>,
        before_artifact_id: Option<&str>,
        limit: u32,
    ) -> Result<LocalAgentArtifactPage, ClientStorageError>;

    async fn read_artifact_data(
        &self,
        owner_user_id: &str,
        artifact_id: &str,
    ) -> Result<Option<Vec<u8>>, ClientStorageError>;

    async fn read_artifact_file(
        &self,
        owner_user_id: &str,
        artifact_id: &str,
    ) -> Result<Option<LocalAgentArtifactFile>, ClientStorageError>;

    async fn delete_artifact(
        &self,
        command: &IdempotentCommand,
        owner_user_id: &str,
        artifact_id: &str,
        now_unix_ms: i64,
    ) -> Result<(), ClientStorageError>;
}

#[async_trait]
pub trait LocalRequirementSurveyStore: Send + Sync {
    async fn get_requirement_plan(
        &self,
        owner_user_id: &str,
        survey_id: &str,
        revision: Option<u64>,
    ) -> Result<Option<chatos_local_agent_protocol::LocalRequirementPlan>, ClientStorageError>;

    async fn save_requirement_plan(
        &self,
        command: &IdempotentCommand,
        save: &chatos_local_agent_protocol::SaveRequirementPlanCommand,
        now_unix_ms: i64,
    ) -> Result<chatos_local_agent_protocol::LocalRequirementPlan, ClientStorageError>;

    async fn confirm_requirement_plan(
        &self,
        command: &IdempotentCommand,
        confirm: &chatos_local_agent_protocol::ConfirmRequirementPlanCommand,
        now_unix_ms: i64,
    ) -> Result<chatos_local_agent_protocol::LocalRequirementPlan, ClientStorageError>;

    async fn create_requirement_survey(
        &self,
        command: &IdempotentCommand,
        survey: &LocalRequirementSurvey,
    ) -> Result<LocalRequirementSurvey, ClientStorageError>;

    async fn list_requirement_surveys(
        &self,
        owner_user_id: &str,
        project_resource_id: Option<&str>,
        status: Option<LocalRequirementSurveyStatus>,
        limit: u32,
    ) -> Result<Vec<LocalRequirementSurvey>, ClientStorageError>;

    async fn get_requirement_survey(
        &self,
        owner_user_id: &str,
        survey_id: &str,
    ) -> Result<Option<LocalRequirementSurvey>, ClientStorageError>;

    async fn resolve_requirement_survey(
        &self,
        command: &IdempotentCommand,
        survey: &LocalRequirementSurvey,
        expected_version: u64,
        run_event_id: &str,
    ) -> Result<LocalRequirementSurveyResolution, ClientStorageError>;
}

pub trait LocalAgentStore:
    LocalAgentRunStore
    + LocalAgentToolStore
    + LocalAgentTaskStore
    + LocalPluginInstallationStore
    + LocalPluginHookBundleStore
    + LocalHookEventStore
    + LocalConversationStore
    + LocalConversationRuntimeSettingsStore
    + LocalNotepadStore
    + LocalRemoteConnectionStore
    + LocalAgentArtifactStore
    + LocalRequirementSurveyStore
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
        + LocalPluginHookBundleStore
        + LocalHookEventStore
        + LocalConversationStore
        + LocalConversationRuntimeSettingsStore
        + LocalNotepadStore
        + LocalRemoteConnectionStore
        + LocalAgentArtifactStore
        + LocalRequirementSurveyStore
        + LocalCapabilitySnapshotStore
        + LocalModelConfigSnapshotStore
        + LocalMemoryOutboxStore
        + LocalMemoryContextCacheStore
{
}
