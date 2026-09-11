// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use async_trait::async_trait;
use chatos_client_storage::{
    AgentEventStateRecord, AgentRunStateRecord, ClientStorage, PutRecord, RecordMetadata,
    RecordQuery, RecordScope, SecretReference, SqliteBootstrapProfile, SqliteClientStorage,
    StorageEncryptionKey, StorageResult, StorageTransaction, TransactionRepositories,
};
use chatos_local_agent_protocol::{
    ContextStrategy, LocalAgentEvent, LocalAgentEventStatus, LocalAgentEventType, LocalAgentRun,
    LocalAgentRunStatus,
};
use chrono::Utc;

fn metadata(id: &str) -> RecordMetadata {
    RecordMetadata {
        id: id.to_string(),
        scope: RecordScope {
            owner_user_id: "user-1".to_string(),
        },
        origin_device_id: "device-1".to_string(),
        revision: 0,
        created_at: Utc::now(),
        updated_at: Utc::now(),
    }
}

fn run() -> AgentRunStateRecord {
    let now = Utc::now();
    AgentRunStateRecord {
        metadata: metadata("run-1"),
        run: LocalAgentRun {
            run_id: "run-1".to_string(),
            profile_key: "task_runner".to_string(),
            owner_user_id: "user-1".to_string(),
            owner_entity_type: "task".to_string(),
            owner_entity_id: "task-1".to_string(),
            project_id: Some("project-1".to_string()),
            status: LocalAgentRunStatus::Queued,
            version: 1,
            step_seq: 0,
            iteration: 0,
            retry_count: 0,
            model_config_id: "model-1".to_string(),
            model_config_revision: 1,
            model_runtime_snapshot: serde_json::json!({"provider": "openai"}),
            context_strategy: ContextStrategy::ProviderNative,
            prompt_revision: "prompt-1".to_string(),
            capability_snapshot_ref: "capability-snapshot-1".to_string(),
            pending_batch_id: None,
            pending_interaction: None,
            terminal_outcome: None,
            deadline_at: None,
            created_at: now,
            updated_at: now,
        },
    }
}

fn event() -> AgentEventStateRecord {
    AgentEventStateRecord {
        metadata: metadata("event-1"),
        event: LocalAgentEvent {
            event_id: "event-1".to_string(),
            run_id: "run-1".to_string(),
            event_type: LocalAgentEventType::RunStarted,
            expected_version: 1,
            available_at: Utc::now(),
            status: LocalAgentEventStatus::Pending,
            attempt_count: 0,
            claimed_by_device_id: None,
            claim_token: None,
            claim_until: None,
            causation_id: "turn-1".to_string(),
            correlation_id: "task-1".to_string(),
            bounded_payload: serde_json::Value::Null,
            last_error: None,
        },
    }
}

fn query(id: &str) -> RecordQuery {
    RecordQuery {
        scope: RecordScope {
            owner_user_id: "user-1".to_string(),
        },
        id: id.to_string(),
    }
}

struct SeedAndRead {
    restored_run: Option<AgentRunStateRecord>,
    restored_event: Option<AgentEventStateRecord>,
}

#[async_trait]
impl StorageTransaction for SeedAndRead {
    async fn execute(
        &mut self,
        repositories: &mut dyn TransactionRepositories,
    ) -> StorageResult<()> {
        repositories
            .agent_runs()
            .put(PutRecord {
                record: run(),
                expected_revision: None,
            })
            .await?;
        repositories
            .agent_events()
            .put(PutRecord {
                record: event(),
                expected_revision: None,
            })
            .await?;
        self.restored_run = repositories.agent_runs().get(&query("run-1")).await?;
        self.restored_event = repositories.agent_events().get(&query("event-1")).await?;
        Ok(())
    }
}

#[tokio::test]
async fn run_and_start_event_commit_in_one_storage_transaction() {
    let directory = tempfile::tempdir().unwrap();
    let storage = SqliteClientStorage::open(
        &SqliteBootstrapProfile {
            database_path: directory.path().join("client.sqlite3"),
            encryption_secret: SecretReference::new("test:sqlite-key").unwrap(),
        },
        &StorageEncryptionKey::new([42; 32]),
    )
    .await
    .unwrap();
    let mut operation = SeedAndRead {
        restored_run: None,
        restored_event: None,
    };
    storage.transaction(&mut operation).await.unwrap();

    assert_eq!(operation.restored_run.unwrap().run.run_id, "run-1");
    assert_eq!(
        operation.restored_event.unwrap().event.event_type,
        LocalAgentEventType::RunStarted
    );
}

#[tokio::test]
async fn mismatched_protocol_and_storage_identity_is_rejected() {
    let directory = tempfile::tempdir().unwrap();
    let storage = SqliteClientStorage::open(
        &SqliteBootstrapProfile {
            database_path: directory.path().join("client.sqlite3"),
            encryption_secret: SecretReference::new("test:sqlite-key").unwrap(),
        },
        &StorageEncryptionKey::new([42; 32]),
    )
    .await
    .unwrap();
    let mut mismatched = run();
    mismatched.metadata.id = "different-run".to_string();
    struct Store(Option<AgentRunStateRecord>);
    #[async_trait]
    impl StorageTransaction for Store {
        async fn execute(
            &mut self,
            repositories: &mut dyn TransactionRepositories,
        ) -> StorageResult<()> {
            repositories
                .agent_runs()
                .put(PutRecord {
                    record: self.0.take().unwrap(),
                    expected_revision: None,
                })
                .await?;
            Ok(())
        }
    }
    assert!(storage
        .transaction(&mut Store(Some(mismatched)))
        .await
        .is_err());
}
