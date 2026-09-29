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
    #[error("decode durable Memory record failed: {0}")]
    Decode(String),
}

#[derive(Clone)]
pub struct LocalMemorySyncWorker {
    store: Arc<dyn LocalMemoryOutboxStore>,
    remote: Arc<dyn MemoryRecordWriter>,
    clock: SyncClock,
    lease_duration: Duration,
}

impl LocalMemorySyncWorker {
    pub fn new(
        store: Arc<dyn LocalMemoryOutboxStore>,
        remote: Arc<dyn MemoryRecordWriter>,
    ) -> Self {
        Self::with_clock(store, remote, Arc::new(system_now_unix_ms))
    }

    pub fn with_clock(
        store: Arc<dyn LocalMemoryOutboxStore>,
        remote: Arc<dyn MemoryRecordWriter>,
        clock: SyncClock,
    ) -> Self {
        Self {
            store,
            remote,
            clock,
            lease_duration: Duration::from_secs(30),
        }
    }

    pub async fn run_once(&self) -> Result<MemorySyncTick, LocalMemorySyncError> {
        let now = (self.clock)().map_err(LocalMemorySyncError::Clock)?;
        let lease_ms = i64::try_from(self.lease_duration.as_millis()).unwrap_or(i64::MAX);
        let claim_token = Uuid::new_v4().to_string();
        let Some(record) = self
            .store
            .claim_next_memory_record(&claim_token, now, now.saturating_add(lease_ms))
            .await?
        else {
            return Ok(MemorySyncTick::Idle);
        };
        let input: SaveRecordInput = serde_json::from_value(record.payload.clone())
            .map_err(|error| LocalMemorySyncError::Decode(error.to_string()))?;
        match self.remote.save_record(input).await {
            Ok(()) => {
                self.store
                    .complete_memory_record(
                        &record.source_id,
                        &record.record_id,
                        &claim_token,
                        record.version,
                        now,
                    )
                    .await?;
                Ok(MemorySyncTick::Synced {
                    record_id: record.record_id,
                })
            }
            Err(error) => {
                let retry_at = now.saturating_add(retry_delay_ms(record.attempt_count));
                self.store
                    .retry_memory_record(
                        &record.source_id,
                        &record.record_id,
                        &claim_token,
                        record.version,
                        &error,
                        retry_at,
                        now,
                    )
                    .await?;
                Ok(MemorySyncTick::RetryScheduled {
                    record_id: record.record_id,
                    retry_at,
                })
            }
        }
    }

    pub async fn next_retry_at(&self) -> Result<Option<i64>, LocalMemorySyncError> {
        Ok(self.store.next_memory_retry_at().await?)
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

#[cfg(test)]
mod tests {
    use super::*;
    use chatos_ai_runtime::SaveRecordInput;
    use chatos_client_storage::SqliteClientStorage;
    use memory_engine_sdk::ComposeContextMeta;
    use serde_json::json;
    use std::sync::Mutex;

    struct RemoteWriter {
        fail: bool,
        records: Mutex<Vec<SaveRecordInput>>,
    }

    #[async_trait]
    impl MemoryRecordWriter for RemoteWriter {
        async fn save_record(&self, input: SaveRecordInput) -> Result<(), String> {
            if self.fail {
                return Err("Memory is offline".to_string());
            }
            self.records.lock().expect("records").push(input);
            Ok(())
        }
    }

    fn user_record() -> SaveRecordInput {
        SaveRecordInput::user_message("conversation-1", "hello")
            .with_message_id("message-1")
            .with_metadata(json!({"tenant_id": "user-1"}))
    }

    #[tokio::test]
    async fn context_cache_round_trips_by_exact_memory_scope() {
        let storage = Arc::new(
            SqliteClientStorage::connect_memory()
                .await
                .expect("storage"),
        );
        let cache = LocalMemoryContextCache::with_clock(storage, Arc::new(|| Ok(1_000)));
        let scope = MemoryScope::thread("user-1", "local_agent", "conversation-1");
        let response = ComposeContextResponse {
            thread_id: "conversation-1".to_string(),
            blocks: Vec::new(),
            recent_records: Vec::new(),
            meta: ComposeContextMeta {
                summary_count: 0,
                recent_record_count: 0,
            },
        };
        cache.store(&scope, &response).await.expect("store");
        let loaded = cache.load(&scope).await.expect("load").expect("cached");
        assert_eq!(loaded.thread_id, "conversation-1");
        assert!(cache
            .load(&MemoryScope::thread(
                "user-1",
                "local_agent",
                "conversation-2"
            ))
            .await
            .expect("missing")
            .is_none());
    }

    #[tokio::test]
    async fn local_writer_enqueues_before_remote_sync() {
        let storage = Arc::new(
            SqliteClientStorage::connect_memory()
                .await
                .expect("storage"),
        );
        let writer = LocalMemoryOutboxWriter::with_clock(
            storage.clone(),
            "local_agent",
            Arc::new(|| Ok(1_000)),
        )
        .expect("writer");
        writer.save_record(user_record()).await.expect("enqueue");

        let queued = storage
            .claim_next_memory_record("claim-1", 2_000, 12_000)
            .await
            .expect("claim")
            .expect("record");
        assert_eq!(queued.record_id, "message-1");
        assert_eq!(queued.tenant_id, "user-1");
        assert_eq!(queued.thread_id, "conversation-1");
        assert_eq!(queued.payload["content"], "hello");
    }

    #[tokio::test]
    async fn sync_worker_completes_success_and_schedules_network_failure() {
        let success_store = Arc::new(
            SqliteClientStorage::connect_memory()
                .await
                .expect("storage"),
        );
        let success_writer = LocalMemoryOutboxWriter::with_clock(
            success_store.clone(),
            "local_agent",
            Arc::new(|| Ok(1_000)),
        )
        .expect("writer");
        success_writer
            .save_record(user_record())
            .await
            .expect("enqueue");
        let remote = Arc::new(RemoteWriter {
            fail: false,
            records: Mutex::new(Vec::new()),
        });
        let worker = LocalMemorySyncWorker::with_clock(
            success_store,
            remote.clone(),
            Arc::new(|| Ok(2_000)),
        );
        assert_eq!(
            worker.run_once().await.expect("sync"),
            MemorySyncTick::Synced {
                record_id: "message-1".to_string()
            }
        );
        assert_eq!(remote.records.lock().expect("records").len(), 1);
        assert_eq!(worker.run_once().await.expect("idle"), MemorySyncTick::Idle);

        let failure_store = Arc::new(
            SqliteClientStorage::connect_memory()
                .await
                .expect("storage"),
        );
        let failure_writer = LocalMemoryOutboxWriter::with_clock(
            failure_store.clone(),
            "local_agent",
            Arc::new(|| Ok(1_000)),
        )
        .expect("writer");
        failure_writer
            .save_record(user_record())
            .await
            .expect("local enqueue remains available");
        let worker = LocalMemorySyncWorker::with_clock(
            failure_store,
            Arc::new(RemoteWriter {
                fail: true,
                records: Mutex::new(Vec::new()),
            }),
            Arc::new(|| Ok(2_000)),
        );
        assert_eq!(
            worker.run_once().await.expect("schedule retry"),
            MemorySyncTick::RetryScheduled {
                record_id: "message-1".to_string(),
                retry_at: 7_000,
            }
        );
        assert_eq!(worker.next_retry_at().await.expect("deadline"), Some(7_000));
    }
}
