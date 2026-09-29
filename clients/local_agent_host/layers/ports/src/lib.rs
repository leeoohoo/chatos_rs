// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

//! Persistence interfaces owned by the Local Agent application layer.

use async_trait::async_trait;
use chatos_local_agent_protocol::{
    CreateTaskGraphCommand, LocalAgentEventRecord, LocalAgentRunClaim, LocalAgentRunRecord,
    LocalAgentRunStatus, LocalAgentToolBatch, LocalAgentToolClaim, LocalAgentToolCommitResult,
    LocalAgentToolOutcome, LocalPluginInstallationRecord, LocalPluginInstallationSpec,
    LocalTaskGraph,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

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

    async fn recover_expired_claims(&self, now_unix_ms: i64) -> Result<u64, ClientStorageError>;

    async fn claim_next_run(
        &self,
        command: &IdempotentCommand,
        worker_id: &str,
        claim_token: &str,
        now_unix_ms: i64,
        claim_until_unix_ms: i64,
        event_id: &str,
    ) -> Result<Option<LocalAgentRunClaim>, ClientStorageError>;

    async fn next_retry_at(&self) -> Result<Option<i64>, ClientStorageError>;

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

    async fn cancel_run(
        &self,
        command: &IdempotentCommand,
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
        graph_id: &str,
    ) -> Result<Option<LocalTaskGraph>, ClientStorageError>;

    async fn list_task_runs(
        &self,
        task_id: &str,
        limit: u32,
    ) -> Result<Vec<LocalAgentRunRecord>, ClientStorageError>;

    async fn start_next_task_run(
        &self,
        run_id: &str,
        event_id: &str,
        now_unix_ms: i64,
    ) -> Result<Option<LocalAgentRunRecord>, ClientStorageError>;

    #[allow(clippy::too_many_arguments)]
    async fn cancel_task(
        &self,
        command: &IdempotentCommand,
        task_id: &str,
        expected_version: Option<u64>,
        reason: &str,
        run_event_id: &str,
        now_unix_ms: i64,
    ) -> Result<LocalTaskGraph, ClientStorageError>;

    async fn retry_task(
        &self,
        command: &IdempotentCommand,
        task_id: &str,
        expected_version: u64,
        now_unix_ms: i64,
    ) -> Result<LocalTaskGraph, ClientStorageError>;

    #[allow(clippy::too_many_arguments)]
    async fn restart_task(
        &self,
        command: &IdempotentCommand,
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
        now_unix_ms: i64,
    ) -> Result<u64, ClientStorageError>;

    #[allow(clippy::too_many_arguments)]
    async fn claim_next_tool(
        &self,
        command: &IdempotentCommand,
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
        invocation_id: &str,
        claim_token: &str,
        expected_version: u64,
        outcome: &LocalAgentToolOutcome,
        event_id: &str,
        batch_event_id: &str,
        now_unix_ms: i64,
    ) -> Result<LocalAgentToolCommitResult, ClientStorageError>;
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
        installation_id: &str,
    ) -> Result<Option<LocalPluginInstallationRecord>, ClientStorageError>;

    async fn list_plugin_installations(
        &self,
        owner_user_id: &str,
        limit: u32,
    ) -> Result<Vec<LocalPluginInstallationRecord>, ClientStorageError>;

    async fn remove_plugin_installation(
        &self,
        command: &IdempotentCommand,
        installation_id: &str,
        expected_version: u64,
        now_unix_ms: i64,
    ) -> Result<LocalPluginInstallationRecord, ClientStorageError>;
}

pub trait LocalAgentStore:
    LocalAgentRunStore + LocalAgentToolStore + LocalAgentTaskStore + LocalPluginInstallationStore
{
}

impl<T> LocalAgentStore for T where
    T: LocalAgentRunStore
        + LocalAgentToolStore
        + LocalAgentTaskStore
        + LocalPluginInstallationStore
{
}
