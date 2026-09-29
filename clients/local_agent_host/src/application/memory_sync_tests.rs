// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::memory_sync::*;
use async_trait::async_trait;
use chatos_ai_runtime::{MemoryContextCache, MemoryRecordWriter, MemoryScope, SaveRecordInput};
use chatos_client_storage::SqliteClientStorage;
use chatos_local_agent_ports::LocalMemoryOutboxStore;
use memory_engine_sdk::{ComposeContextMeta, ComposeContextResponse};
use serde_json::json;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc, Mutex,
};

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
    let writer =
        LocalMemoryOutboxWriter::with_clock(storage.clone(), "local_agent", Arc::new(|| Ok(1_000)))
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
    let worker =
        LocalMemorySyncWorker::with_clock(success_store, remote.clone(), Arc::new(|| Ok(2_000)));
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

#[tokio::test]
async fn retry_deadline_uses_request_completion_time() {
    let storage = Arc::new(
        SqliteClientStorage::connect_memory()
            .await
            .expect("storage"),
    );
    LocalMemoryOutboxWriter::with_clock(storage.clone(), "local_agent", Arc::new(|| Ok(1_000)))
        .expect("writer")
        .save_record(user_record())
        .await
        .expect("enqueue");
    let calls = Arc::new(AtomicUsize::new(0));
    let clock_calls = Arc::clone(&calls);
    let worker = LocalMemorySyncWorker::with_clock(
        storage,
        Arc::new(RemoteWriter {
            fail: true,
            records: Mutex::new(Vec::new()),
        }),
        Arc::new(move || {
            Ok(if clock_calls.fetch_add(1, Ordering::SeqCst) == 0 {
                2_000
            } else {
                10_000
            })
        }),
    );
    assert_eq!(
        worker.run_once().await.expect("retry"),
        MemorySyncTick::RetryScheduled {
            record_id: "message-1".to_string(),
            retry_at: 15_000,
        }
    );
}

#[tokio::test]
async fn malformed_local_record_becomes_visible_retry_instead_of_stopping_worker() {
    let storage = Arc::new(
        SqliteClientStorage::connect_memory()
            .await
            .expect("storage"),
    );
    storage
        .enqueue_memory_record(
            "message-bad",
            "user-1",
            "local_agent",
            "conversation-1",
            &json!({"invalid": true}),
            1_000,
        )
        .await
        .expect("enqueue malformed durable payload");
    let remote = Arc::new(RemoteWriter {
        fail: false,
        records: Mutex::new(Vec::new()),
    });
    let worker =
        LocalMemorySyncWorker::with_clock(storage.clone(), remote.clone(), Arc::new(|| Ok(2_000)));
    assert_eq!(
        worker.run_once().await.expect("retry malformed record"),
        MemorySyncTick::RetryScheduled {
            record_id: "message-bad".to_string(),
            retry_at: 7_000,
        }
    );
    assert!(remote.records.lock().expect("records").is_empty());
    let status = storage
        .get_memory_sync_status("user-1", "local_agent")
        .await
        .expect("status");
    assert_eq!(status.retry_scheduled_count, 1);
    assert!(status
        .last_error
        .as_deref()
        .is_some_and(|error| error.contains("decode durable Memory record failed")));
}
