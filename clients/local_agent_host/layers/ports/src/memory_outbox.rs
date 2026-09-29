// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::ClientStorageError;
use async_trait::async_trait;
use chatos_local_agent_protocol::LocalMemorySyncStatus;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LocalMemoryOutboxStatus {
    Pending,
    Syncing,
    RetryScheduled,
    Synced,
}

impl LocalMemoryOutboxStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Syncing => "syncing",
            Self::RetryScheduled => "retry_scheduled",
            Self::Synced => "synced",
        }
    }
}

impl std::str::FromStr for LocalMemoryOutboxStatus {
    type Err = ClientStorageError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "pending" => Ok(Self::Pending),
            "syncing" => Ok(Self::Syncing),
            "retry_scheduled" => Ok(Self::RetryScheduled),
            "synced" => Ok(Self::Synced),
            _ => Err(ClientStorageError::InvalidState(format!(
                "unknown Memory outbox status: {value}"
            ))),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LocalMemoryOutboxRecord {
    pub record_id: String,
    pub tenant_id: String,
    pub source_id: String,
    pub thread_id: String,
    pub payload: Value,
    pub status: LocalMemoryOutboxStatus,
    pub attempt_count: u32,
    pub version: u64,
    pub claim_token: Option<String>,
    pub claim_until_unix_ms: Option<i64>,
    pub next_attempt_at_unix_ms: Option<i64>,
    pub last_error: Option<String>,
    pub created_at_unix_ms: i64,
    pub updated_at_unix_ms: i64,
}

#[async_trait]
pub trait LocalMemoryOutboxStore: Send + Sync {
    async fn enqueue_memory_record(
        &self,
        record_id: &str,
        tenant_id: &str,
        source_id: &str,
        thread_id: &str,
        payload: &Value,
        now_unix_ms: i64,
    ) -> Result<LocalMemoryOutboxRecord, ClientStorageError>;

    async fn claim_next_memory_record(
        &self,
        tenant_id: &str,
        claim_token: &str,
        now_unix_ms: i64,
        claim_until_unix_ms: i64,
    ) -> Result<Option<LocalMemoryOutboxRecord>, ClientStorageError>;

    async fn complete_memory_record(
        &self,
        source_id: &str,
        record_id: &str,
        claim_token: &str,
        expected_version: u64,
        now_unix_ms: i64,
    ) -> Result<LocalMemoryOutboxRecord, ClientStorageError>;

    #[allow(clippy::too_many_arguments)]
    async fn retry_memory_record(
        &self,
        source_id: &str,
        record_id: &str,
        claim_token: &str,
        expected_version: u64,
        error: &str,
        next_attempt_at_unix_ms: i64,
        now_unix_ms: i64,
    ) -> Result<LocalMemoryOutboxRecord, ClientStorageError>;

    async fn next_memory_retry_at(
        &self,
        tenant_id: &str,
    ) -> Result<Option<i64>, ClientStorageError>;

    async fn get_memory_sync_status(
        &self,
        tenant_id: &str,
        source_id: &str,
    ) -> Result<LocalMemorySyncStatus, ClientStorageError>;
}
