// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use async_trait::async_trait;
use chatos_ai_runtime::{MemoryContextCache, MemoryRecordWriter, MemoryScope, SaveRecordInput};
use chatos_local_agent_ports::{
    ClientStorageError, LocalMemoryContextCacheStore, LocalMemoryOutboxStore,
};
use memory_engine_sdk::ComposeContextResponse;
use serde_json::Value;
use std::{sync::Arc, time::Duration};
use thiserror::Error;
use uuid::Uuid;

type SyncClock = Arc<dyn Fn() -> Result<i64, String> + Send + Sync>;

#[derive(Clone)]
pub struct LocalMemoryContextCache {
    store: Arc<dyn LocalMemoryContextCacheStore>,
    clock: SyncClock,
}

impl LocalMemoryContextCache {
    pub fn new(store: Arc<dyn LocalMemoryContextCacheStore>) -> Self {
        Self::with_clock(store, Arc::new(system_now_unix_ms))
    }

    pub fn with_clock(store: Arc<dyn LocalMemoryContextCacheStore>, clock: SyncClock) -> Self {
        Self { store, clock }
    }
}

#[async_trait]
impl MemoryContextCache for LocalMemoryContextCache {
    async fn load(&self, scope: &MemoryScope) -> Result<Option<ComposeContextResponse>, String> {
        let key = scope_cache_key(scope)?;
        self.store
            .get_memory_context_cache(&key)
            .await
            .map_err(|error| error.to_string())?
            .map(|value| {
                serde_json::from_value(value)
                    .map_err(|error| format!("decode local Memory context cache failed: {error}"))
            })
            .transpose()
    }

    async fn store(
        &self,
        scope: &MemoryScope,
        response: &ComposeContextResponse,
    ) -> Result<(), String> {
        let key = scope_cache_key(scope)?;
        let response = serde_json::to_value(response)
            .map_err(|error| format!("encode local Memory context cache failed: {error}"))?;
        self.store
            .put_memory_context_cache(
                &key,
                &scope.tenant_id,
                &scope.source_id,
                &scope.thread_id,
                &response,
                (self.clock)()?,
            )
            .await
            .map_err(|error| error.to_string())
    }
}

#[derive(Clone)]
pub struct LocalMemoryOutboxWriter {
    store: Arc<dyn LocalMemoryOutboxStore>,
    source_id: String,
    clock: SyncClock,
}

impl LocalMemoryOutboxWriter {
    pub fn new(
        store: Arc<dyn LocalMemoryOutboxStore>,
        source_id: impl Into<String>,
    ) -> Result<Self, String> {
        Self::with_clock(store, source_id, Arc::new(system_now_unix_ms))
    }

    pub fn with_clock(
        store: Arc<dyn LocalMemoryOutboxStore>,
        source_id: impl Into<String>,
        clock: SyncClock,
    ) -> Result<Self, String> {
        let source_id = source_id.into().trim().to_string();
        if source_id.is_empty() {
            return Err("Memory outbox source_id must not be empty".to_string());
        }
        Ok(Self {
            store,
            source_id,
            clock,
        })
    }
}

#[async_trait]
impl MemoryRecordWriter for LocalMemoryOutboxWriter {
    async fn save_record(&self, input: SaveRecordInput) -> Result<(), String> {
        let record_id = required(input.message_id.as_deref(), "message_id")?.to_string();
        let thread_id =
            required(Some(input.conversation_id.as_str()), "conversation_id")?.to_string();
        let tenant_id = input
            .metadata
            .as_ref()
            .and_then(|metadata| metadata.get("tenant_id"))
            .and_then(Value::as_str);
        let tenant_id = required(tenant_id, "metadata.tenant_id")?.to_string();
        let payload = serde_json::to_value(input)
            .map_err(|error| format!("serialize Memory outbox record failed: {error}"))?;
        self.store
            .enqueue_memory_record(
                &record_id,
                &tenant_id,
                &self.source_id,
                &thread_id,
                &payload,
                (self.clock)()?,
            )
            .await
            .map(|_| ())
            .map_err(|error| error.to_string())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MemorySyncTick {
    Idle,
    Synced { record_id: String },
    RetryScheduled { record_id: String, retry_at: i64 },
}

#[derive(Debug, Error)]
pub enum LocalMemorySyncError {
    #[error(transparent)]
    Storage(#[from] ClientStorageError),
    #[error("Memory sync clock failed: {0}")]
    Clock(String),
}

#[derive(Clone)]
pub struct LocalMemorySyncWorker {
    store: Arc<dyn LocalMemoryOutboxStore>,
    remote: Arc<dyn MemoryRecordWriter>,
    tenant_id: String,
    clock: SyncClock,
    lease_duration: Duration,
}

impl LocalMemorySyncWorker {
    pub fn new(
        store: Arc<dyn LocalMemoryOutboxStore>,
        remote: Arc<dyn MemoryRecordWriter>,
        tenant_id: impl Into<String>,
    ) -> Result<Self, String> {
        Self::with_clock(store, remote, tenant_id, Arc::new(system_now_unix_ms))
    }

    pub fn with_clock(
        store: Arc<dyn LocalMemoryOutboxStore>,
        remote: Arc<dyn MemoryRecordWriter>,
        tenant_id: impl Into<String>,
        clock: SyncClock,
    ) -> Result<Self, String> {
        let tenant_id = tenant_id.into();
        let tenant_id = tenant_id.trim();
        if tenant_id.is_empty() || tenant_id.len() > 256 || tenant_id.chars().any(char::is_control)
        {
            return Err("Memory sync tenant id must be 1..=256 non-control characters".to_string());
        }
        Ok(Self {
            store,
            remote,
            tenant_id: tenant_id.to_string(),
            clock,
            lease_duration: Duration::from_secs(60),
        })
    }

    pub fn with_lease_duration(mut self, lease_duration: Duration) -> Result<Self, String> {
        if lease_duration < Duration::from_secs(1) {
            return Err("Memory sync lease must be at least one second".to_string());
        }
        self.lease_duration = lease_duration;
        Ok(self)
    }

    pub async fn run_once(&self) -> Result<MemorySyncTick, LocalMemorySyncError> {
        let now = (self.clock)().map_err(LocalMemorySyncError::Clock)?;
        let lease_ms = i64::try_from(self.lease_duration.as_millis()).unwrap_or(i64::MAX);
        let claim_token = Uuid::new_v4().to_string();
        let Some(record) = self
            .store
            .claim_next_memory_record(
                &self.tenant_id,
                &claim_token,
                now,
                now.saturating_add(lease_ms),
            )
            .await?
        else {
            return Ok(MemorySyncTick::Idle);
        };
        let input: SaveRecordInput = match serde_json::from_value(record.payload.clone()) {
            Ok(input) => input,
            Err(error) => {
                let completed_at = (self.clock)().map_err(LocalMemorySyncError::Clock)?;
                return self
                    .schedule_retry(
                        &record,
                        &claim_token,
                        &format!("decode durable Memory record failed: {error}"),
                        completed_at,
                    )
                    .await;
            }
        };
        let result = self.remote.save_record(input).await;
        let completed_at = (self.clock)().map_err(LocalMemorySyncError::Clock)?;
        match result {
            Ok(()) => {
                self.store
                    .complete_memory_record(
                        &record.source_id,
                        &record.record_id,
                        &claim_token,
                        record.version,
                        completed_at,
                    )
                    .await?;
                Ok(MemorySyncTick::Synced {
                    record_id: record.record_id,
                })
            }
            Err(error) => {
                self.schedule_retry(&record, &claim_token, &error, completed_at)
                    .await
            }
        }
    }

    async fn schedule_retry(
        &self,
        record: &chatos_local_agent_ports::LocalMemoryOutboxRecord,
        claim_token: &str,
        error: &str,
        completed_at_unix_ms: i64,
    ) -> Result<MemorySyncTick, LocalMemorySyncError> {
        let retry_at = completed_at_unix_ms.saturating_add(retry_delay_ms(record.attempt_count));
        self.store
            .retry_memory_record(
                &record.source_id,
                &record.record_id,
                claim_token,
                record.version,
                error,
                retry_at,
                completed_at_unix_ms,
            )
            .await?;
        Ok(MemorySyncTick::RetryScheduled {
            record_id: record.record_id.clone(),
            retry_at,
        })
    }

    pub async fn next_retry_at(&self) -> Result<Option<i64>, LocalMemorySyncError> {
        Ok(self.store.next_memory_retry_at(&self.tenant_id).await?)
    }
}

fn retry_delay_ms(completed_attempts: u32) -> i64 {
    let shift = completed_attempts.min(9);
    5_000_i64
        .saturating_mul(1_i64 << shift)
        .min(30 * 60 * 1_000)
}

fn scope_cache_key(scope: &MemoryScope) -> Result<String, String> {
    serde_json::to_string(scope)
        .map_err(|error| format!("encode Memory scope cache key failed: {error}"))
}

fn required<'a>(value: Option<&'a str>, label: &str) -> Result<&'a str, String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| format!("Memory record {label} is required"))
}

fn system_now_unix_ms() -> Result<i64, String> {
    let duration = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|error| error.to_string())?;
    i64::try_from(duration.as_millis()).map_err(|_| "Unix time overflow".to_string())
}
