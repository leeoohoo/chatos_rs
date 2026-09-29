// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use async_trait::async_trait;
use chatos_local_agent_protocol::{
    LocalAgentEventRecord, LocalAgentRunClaim, LocalAgentRunRecord, LocalAgentRunStatus,
    LocalAgentToolBatch,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ClientStorageError {
    #[error("client storage database error: {0}")]
    Database(#[from] sqlx::Error),
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
    pub next_attempt_at_unix_ms: Option<i64>,
    pub pending_tool_batch: Option<Value>,
    pub tool_batch: Option<LocalAgentToolBatch>,
    pub checkpoint: Option<Value>,
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
