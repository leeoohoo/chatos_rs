// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::{collections::VecDeque, sync::Arc, time::Duration};

use async_trait::async_trait;
use chatos_client_storage::{
    AgentMessageStateRecord, AgentUiEventCursorQuery, ClientStorage, RecordQuery, RecordScope,
    SecretReference, SqliteBootstrapProfile, SqliteClientStorage, StorageEncryptionKey,
    StorageResult, StorageTransaction, SyncOutboxStateRecord, TransactionRepositories,
};
use chatos_local_agent_protocol::{
    AgentMessage, AgentMessageRole, LocalAgentUiEvent, LocalAgentUiEventPayload, MemorySyncStatus,
    MessageMode, SyncOutboxStatus,
};
use chatos_local_agent_runtime::{
    claim_memory_sync_batch, record_semantic_message, ClaimMemorySyncBatchRequest, MemorySyncApi,
    MemorySyncApiReceipt, MemorySyncApiRequest, MemorySyncPolicy, MemorySynchronizer,
    RecordSemanticMessageRequest,
};
use chrono::{Duration as ChronoDuration, Utc};
use serde_json::json;
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

#[derive(Clone, Copy)]
enum ApiBehavior {
    Success,
    Failure,
    Partial,
}

struct MockMemoryApi {
    behavior: Mutex<VecDeque<ApiBehavior>>,
    requests: Mutex<Vec<MemorySyncApiRequest>>,
}

#[async_trait]
impl MemorySyncApi for MockMemoryApi {
    async fn batch_sync(
        &self,
        request: MemorySyncApiRequest,
        _cancellation: CancellationToken,
    ) -> Result<MemorySyncApiReceipt, String> {
        let behavior = self
            .behavior
            .lock()
            .await
            .pop_front()
            .unwrap_or(ApiBehavior::Success);
        let receipt = MemorySyncApiReceipt {
            thread_id: request.thread_id.clone(),
            received_count: if matches!(behavior, ApiBehavior::Partial) {
                request.records.len().saturating_sub(1)
            } else {
                request.records.len()
            },
            upserted_count: request.records.len(),
        };
        self.requests.lock().await.push(request);
        match behavior {
            ApiBehavior::Success | ApiBehavior::Partial => Ok(receipt),
            ApiBehavior::Failure => Err("Memory Engine unavailable".to_string()),
        }
    }
}

fn scope() -> RecordScope {
    RecordScope {
        owner_user_id: "user-1".to_string(),
    }
}

async fn storage() -> (tempfile::TempDir, Arc<SqliteClientStorage>) {
    let directory = tempfile::tempdir().unwrap();
    let storage = Arc::new(
        SqliteClientStorage::open(
            &SqliteBootstrapProfile {
                database_path: directory.path().join("client.sqlite3"),
                encryption_secret: SecretReference::new("test:memory-sync-key").unwrap(),
            },
            &StorageEncryptionKey::new([31; 32]),
        )
        .await
        .unwrap(),
    );
    (directory, storage)
}

fn message(id: &str, thread_id: &str, sequence: u64, now: chrono::DateTime<Utc>) -> AgentMessage {
    AgentMessage {
        record_id: id.to_string(),
        run_id: "run-1".to_string(),
        thread_id: thread_id.to_string(),
        turn_id: format!("turn-{sequence}"),
        sequence,
        role: AgentMessageRole::Assistant,
        content: Some(format!("message {sequence}")),
        reasoning: None,
        structured_payload: Some(json!({"sequence": sequence})),
        tool_call_id: None,
        response_id: Some(format!("response-{sequence}")),
        message_mode: MessageMode::Semantic,
        message_source: "local_agent".to_string(),
        memory_sync_status: MemorySyncStatus::Pending,
        created_at: now,
    }
}

async fn record(storage: &SqliteClientStorage, message: AgentMessage, now: chrono::DateTime<Utc>) {
    record_semantic_message(
        storage,
        RecordSemanticMessageRequest {
            scope: scope(),
            message,
            origin_device_id: "device-1".to_string(),
            now,
        },
    )
    .await
    .unwrap();
}

fn policy(maximum_attempts: u32) -> MemorySyncPolicy {
    MemorySyncPolicy {
        batch_limit: 100,
        lease_duration: Duration::from_secs(1),
        maximum_attempts,
        base_retry_delay: Duration::from_secs(1),
        maximum_retry_delay: Duration::from_secs(8),
    }
}

struct ReadUiEvents(Vec<LocalAgentUiEvent>);

#[async_trait]
impl StorageTransaction for ReadUiEvents {
    async fn execute(
        &mut self,
        repositories: &mut dyn TransactionRepositories,
    ) -> StorageResult<()> {
        self.0 = repositories
            .agent_ui_events()
            .list_after(&AgentUiEventCursorQuery {
                scope: scope(),
                after_seq: 0,
                limit: 100,
            })
            .await?
            .records
            .into_iter()
            .map(|record| record.event)
            .collect();
        Ok(())
    }
}

async fn read_ui_events(storage: &SqliteClientStorage) -> Vec<LocalAgentUiEvent> {
    let mut operation = ReadUiEvents(Vec::new());
    storage.transaction(&mut operation).await.unwrap();
    operation.0
}

#[tokio::test]
async fn successful_batches_are_grouped_by_thread_and_not_sent_twice() {
    let now = Utc::now();
    let (_directory, storage) = storage().await;
    record(
        storage.as_ref(),
        message("message-1", "thread-1", 1, now),
        now,
    )
    .await;
    record(
        storage.as_ref(),
        message("message-2", "thread-1", 2, now),
        now,
    )
    .await;
    let api = Arc::new(MockMemoryApi {
        behavior: Mutex::new(VecDeque::from([ApiBehavior::Success])),
        requests: Mutex::new(Vec::new()),
    });
    let synchronizer =
        MemorySynchronizer::new(api.clone(), "tenant-1", "source-1", policy(3)).unwrap();

    let report = synchronizer
        .sync_once(storage.as_ref(), scope(), now, CancellationToken::new())
        .await
        .unwrap();
    assert_eq!(report.claimed, 2);
    assert_eq!(report.synced, 2);
    assert!(report.errors.is_empty());
    let requests = api.requests.lock().await;
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].thread_id, "thread-1");
    assert_eq!(requests[0].records.len(), 2);
    drop(requests);

    let repeated = synchronizer
        .sync_once(storage.as_ref(), scope(), now, CancellationToken::new())
        .await
        .unwrap();
    assert_eq!(repeated.claimed, 0);

    // Re-recording the same stable semantic identity remains idempotent even
    // after its local sync status changed to succeeded.
    record(
        storage.as_ref(),
        message("message-1", "thread-1", 1, now),
        now,
    )
    .await;
    let events = read_ui_events(storage.as_ref()).await;
    assert_eq!(
        events.len(),
        4,
        "idempotent re-entry emits no duplicate status"
    );
    let LocalAgentUiEventPayload::MemorySync(status) = &events[3].event else {
        panic!("Memory Sync completion must publish its aggregate state");
    };
    assert_eq!(status.pending_count, 0);
    assert_eq!(status.failed_count, 0);
}

#[tokio::test]
async fn transport_failures_back_off_and_eventually_become_explicit_failures() {
    let now = Utc::now();
    let (_directory, storage) = storage().await;
    record(
        storage.as_ref(),
        message("message-1", "thread-1", 1, now),
        now,
    )
    .await;
    let api = Arc::new(MockMemoryApi {
        behavior: Mutex::new(VecDeque::from([ApiBehavior::Failure, ApiBehavior::Failure])),
        requests: Mutex::new(Vec::new()),
    });
    let synchronizer = MemorySynchronizer::new(api, "tenant-1", "source-1", policy(2)).unwrap();

    let first = synchronizer
        .sync_once(storage.as_ref(), scope(), now, CancellationToken::new())
        .await
        .unwrap();
    assert_eq!(first.deferred, 1);
    assert_eq!(first.permanently_failed, 0);

    let second = synchronizer
        .sync_once(
            storage.as_ref(),
            scope(),
            now + ChronoDuration::seconds(2),
            CancellationToken::new(),
        )
        .await
        .unwrap();
    assert_eq!(second.permanently_failed, 1);

    let state = read_state(storage.as_ref(), "message-1").await;
    assert_eq!(state.0.message.memory_sync_status, MemorySyncStatus::Failed);
    assert_eq!(state.1.item.status, SyncOutboxStatus::Failed);
    assert_eq!(state.1.item.attempt_count, 2);
    let events = read_ui_events(storage.as_ref()).await;
    let LocalAgentUiEventPayload::MemorySync(status) = &events.last().unwrap().event else {
        panic!("permanent Memory Sync failure must be visible to the UI");
    };
    assert_eq!(status.pending_count, 0);
    assert_eq!(status.failed_count, 1);
    assert_eq!(
        status.last_error_code.as_deref(),
        Some("memory_sync_failed")
    );
}

#[tokio::test]
async fn expired_in_flight_items_are_reclaimed_without_duplicate_records() {
    let now = Utc::now();
    let (_directory, storage) = storage().await;
    record(
        storage.as_ref(),
        message("message-1", "thread-1", 1, now),
        now,
    )
    .await;
    let first = claim_memory_sync_batch(
        storage.as_ref(),
        ClaimMemorySyncBatchRequest {
            scope: scope(),
            now,
            policy: policy(3),
        },
    )
    .await
    .unwrap();
    assert_eq!(first.records[0].outbox.item.attempt_count, 1);

    let recovered = claim_memory_sync_batch(
        storage.as_ref(),
        ClaimMemorySyncBatchRequest {
            scope: scope(),
            now: now + ChronoDuration::seconds(2),
            policy: policy(3),
        },
    )
    .await
    .unwrap();
    assert_eq!(recovered.records.len(), 1);
    assert_eq!(recovered.records[0].outbox.item.attempt_count, 2);
    assert_eq!(recovered.records[0].message.message.record_id, "message-1");
}

#[tokio::test]
async fn partial_acknowledgement_is_not_treated_as_success() {
    let now = Utc::now();
    let (_directory, storage) = storage().await;
    record(
        storage.as_ref(),
        message("message-1", "thread-1", 1, now),
        now,
    )
    .await;
    let api = Arc::new(MockMemoryApi {
        behavior: Mutex::new(VecDeque::from([ApiBehavior::Partial])),
        requests: Mutex::new(Vec::new()),
    });
    let synchronizer = MemorySynchronizer::new(api, "tenant-1", "source-1", policy(3)).unwrap();
    let report = synchronizer
        .sync_once(storage.as_ref(), scope(), now, CancellationToken::new())
        .await
        .unwrap();
    assert_eq!(report.synced, 0);
    assert_eq!(report.deferred, 1);
    assert_eq!(
        report.errors,
        vec!["Memory Engine acknowledged a partial batch"]
    );
}

struct ReadState {
    message_id: String,
    message: Option<AgentMessageStateRecord>,
    outbox: Option<SyncOutboxStateRecord>,
}

#[async_trait]
impl StorageTransaction for ReadState {
    async fn execute(
        &mut self,
        repositories: &mut dyn TransactionRepositories,
    ) -> StorageResult<()> {
        self.message = repositories
            .agent_messages()
            .get(&RecordQuery {
                scope: scope(),
                id: self.message_id.clone(),
            })
            .await?;
        let mut cursor = None;
        loop {
            let page = repositories
                .sync_outbox()
                .list(&chatos_client_storage::ListQuery {
                    scope: scope(),
                    cursor: cursor.clone(),
                    limit: chatos_client_storage::ListQuery::MAX_LIMIT,
                })
                .await?;
            if let Some(record) = page
                .records
                .into_iter()
                .find(|record| record.item.record_id == self.message_id)
            {
                self.outbox = Some(record);
                break;
            }
            let Some(next) = page.next_cursor else {
                break;
            };
            cursor = Some(next);
        }
        Ok(())
    }
}

async fn read_state(
    storage: &SqliteClientStorage,
    message_id: &str,
) -> (AgentMessageStateRecord, SyncOutboxStateRecord) {
    let mut state = ReadState {
        message_id: message_id.to_string(),
        message: None,
        outbox: None,
    };
    storage.transaction(&mut state).await.unwrap();
    (state.message.unwrap(), state.outbox.unwrap())
}
