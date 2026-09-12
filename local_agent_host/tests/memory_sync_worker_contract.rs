// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use chatos_client_storage::{
    RecordScope, SecretReference, SqliteBootstrapProfile, SqliteClientStorage, StorageEncryptionKey,
};
use chatos_local_agent_host::LocalAgentMemorySyncWorker;
use chatos_local_agent_protocol::{AgentMessage, AgentMessageRole, MemorySyncStatus, MessageMode};
use chatos_local_agent_runtime::{
    record_semantic_message, MemorySyncApi, MemorySyncApiReceipt, MemorySyncApiRequest,
    MemorySyncPolicy, MemorySynchronizer, RecordSemanticMessageRequest,
};
use chrono::Utc;
use tokio::sync::{Mutex, Notify};
use tokio_util::sync::CancellationToken;

struct RecordingMemoryApi {
    requests: Mutex<Vec<MemorySyncApiRequest>>,
    received: Notify,
}

#[async_trait]
impl MemorySyncApi for RecordingMemoryApi {
    async fn batch_sync(
        &self,
        request: MemorySyncApiRequest,
        _cancellation: CancellationToken,
    ) -> Result<MemorySyncApiReceipt, String> {
        let receipt = MemorySyncApiReceipt {
            thread_id: request.thread_id.clone(),
            received_count: request.records.len(),
            upserted_count: request.records.len(),
        };
        self.requests.lock().await.push(request);
        self.received.notify_one();
        Ok(receipt)
    }
}

#[tokio::test]
async fn host_memory_worker_drains_pending_semantic_records_until_shutdown() {
    let directory = tempfile::tempdir().unwrap();
    let storage = Arc::new(
        SqliteClientStorage::open(
            &SqliteBootstrapProfile {
                database_path: directory.path().join("client.sqlite3"),
                encryption_secret: SecretReference::new("memory-worker-key").unwrap(),
            },
            &StorageEncryptionKey::new([41_u8; 32]),
        )
        .await
        .unwrap(),
    );
    let scope = RecordScope {
        owner_user_id: "user-1".to_string(),
    };
    let now = Utc::now();
    record_semantic_message(
        storage.as_ref(),
        RecordSemanticMessageRequest {
            scope: scope.clone(),
            message: AgentMessage {
                record_id: "message-1".to_string(),
                run_id: "run-1".to_string(),
                thread_id: "thread-1".to_string(),
                turn_id: "turn-1".to_string(),
                sequence: 1,
                role: AgentMessageRole::Assistant,
                content: Some("A durable assistant reply".to_string()),
                reasoning: None,
                structured_payload: None,
                tool_call_id: None,
                response_id: Some("response-1".to_string()),
                message_mode: MessageMode::Semantic,
                message_source: "local_agent".to_string(),
                memory_sync_status: MemorySyncStatus::Pending,
                created_at: now,
            },
            origin_device_id: "device-1".to_string(),
            now,
        },
    )
    .await
    .unwrap();
    let api = Arc::new(RecordingMemoryApi {
        requests: Mutex::new(Vec::new()),
        received: Notify::new(),
    });
    let synchronizer = MemorySynchronizer::new(
        api.clone(),
        "tenant-1",
        "local-agent",
        MemorySyncPolicy {
            batch_limit: 100,
            lease_duration: Duration::from_secs(10),
            maximum_attempts: 3,
            base_retry_delay: Duration::from_secs(1),
            maximum_retry_delay: Duration::from_secs(8),
        },
    )
    .unwrap();
    let worker = LocalAgentMemorySyncWorker::new(storage, scope, synchronizer);
    let shutdown = CancellationToken::new();
    let worker_shutdown = shutdown.clone();
    let running = tokio::spawn(async move { worker.run(worker_shutdown).await });

    api.received.notified().await;
    shutdown.cancel();
    let exit = running.await.unwrap().unwrap();

    assert_eq!(exit.claimed_record_count, 1);
    assert_eq!(exit.synchronized_record_count, 1);
    assert_eq!(exit.permanently_failed_record_count, 0);
    let requests = api.requests.lock().await;
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].thread_id, "thread-1");
    assert_eq!(requests[0].records.len(), 1);
    assert_eq!(requests[0].records[0].id, "message-1");
}
