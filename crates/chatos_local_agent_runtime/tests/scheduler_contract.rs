// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use async_trait::async_trait;
use chatos_client_storage::{
    AgentEventStateRecord, AgentRunStateRecord, ClientStorage, PutRecord, RecordMetadata,
    RecordScope, SecretReference, SqliteBootstrapProfile, SqliteClientStorage,
    StorageEncryptionKey, StorageResult, StorageTransaction, TransactionRepositories,
};
use chatos_local_agent_protocol::{
    ContextStrategy, LocalAgentEvent, LocalAgentEventStatus, LocalAgentEventType, LocalAgentRun,
    LocalAgentRunStatus,
};
use chatos_local_agent_runtime::{
    scan_recoverable_work, DurableScheduler, SchedulerTickRequest, SchedulerTickResult,
};
use chrono::{Duration, Utc};

fn scope() -> RecordScope {
    RecordScope {
        owner_user_id: "user-1".to_string(),
    }
}

struct Seed {
    now: chrono::DateTime<Utc>,
}

#[async_trait]
impl StorageTransaction for Seed {
    async fn execute(
        &mut self,
        repositories: &mut dyn TransactionRepositories,
    ) -> StorageResult<()> {
        let metadata = |id: &str| RecordMetadata {
            id: id.to_string(),
            scope: scope(),
            origin_device_id: "device-1".to_string(),
            revision: 0,
            created_at: self.now,
            updated_at: self.now,
        };
        repositories
            .agent_runs()
            .put(PutRecord {
                record: AgentRunStateRecord {
                    metadata: metadata("run-1"),
                    run: LocalAgentRun {
                        run_id: "run-1".to_string(),
                        profile_key: "main_chat".to_string(),
                        owner_user_id: "user-1".to_string(),
                        owner_entity_type: "conversation".to_string(),
                        owner_entity_id: "conversation-1".to_string(),
                        project_id: None,
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
                        capability_snapshot_ref: "capabilities-1".to_string(),
                        pending_batch_id: None,
                        pending_interaction: None,
                        terminal_outcome: None,
                        deadline_at: None,
                        created_at: self.now,
                        updated_at: self.now,
                    },
                },
                expected_revision: None,
            })
            .await?;
        repositories
            .agent_events()
            .put(PutRecord {
                record: AgentEventStateRecord {
                    metadata: metadata("event-1"),
                    event: LocalAgentEvent {
                        event_id: "event-1".to_string(),
                        run_id: "run-1".to_string(),
                        event_type: LocalAgentEventType::RunStarted,
                        expected_version: 1,
                        available_at: self.now,
                        status: LocalAgentEventStatus::Pending,
                        attempt_count: 0,
                        claimed_by_device_id: None,
                        claim_token: None,
                        claim_until: None,
                        causation_id: "turn-1".to_string(),
                        correlation_id: "conversation-1".to_string(),
                        bounded_payload: serde_json::Value::Null,
                        last_error: None,
                    },
                },
                expected_revision: None,
            })
            .await?;
        Ok(())
    }
}

#[tokio::test]
async fn a_tick_claims_exactly_one_event_without_owning_an_agent_loop() {
    let now = Utc::now();
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
    storage.transaction(&mut Seed { now }).await.unwrap();
    let plan = scan_recoverable_work(&storage, scope(), now).await.unwrap();
    let mut scheduler = DurableScheduler::from_recovery(scope(), &plan);

    let result = scheduler
        .tick(
            &storage,
            SchedulerTickRequest {
                device_id: "device-1".to_string(),
                claim_token: "claim-1".to_string(),
                now,
                claim_ttl: Duration::seconds(30),
                max_attempts: 3,
            },
        )
        .await
        .unwrap();
    let SchedulerTickResult::Claimed(event) = result else {
        panic!("scheduler did not claim the due event");
    };
    assert_eq!(event.event.event_id, "event-1");
    assert_eq!(event.event.attempt_count, 1);

    assert_eq!(
        scheduler
            .tick(
                &storage,
                SchedulerTickRequest {
                    device_id: "device-1".to_string(),
                    claim_token: "claim-2".to_string(),
                    now,
                    claim_ttl: Duration::seconds(30),
                    max_attempts: 3,
                },
            )
            .await
            .unwrap(),
        SchedulerTickResult::Idle {
            next_wake_at: Some(now + Duration::seconds(30)),
        }
    );
}

#[tokio::test]
async fn scheduler_requests_a_storage_rescan_when_a_durable_wake_expires() {
    let now = Utc::now();
    let plan = chatos_local_agent_runtime::RecoveryPlan {
        active_runs: Vec::new(),
        ready_events: Vec::new(),
        next_wake_at: Some(now + Duration::seconds(10)),
        issues: Vec::new(),
    };
    let mut scheduler = DurableScheduler::from_recovery(scope(), &plan);
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

    assert_eq!(
        scheduler
            .tick(
                &storage,
                SchedulerTickRequest {
                    device_id: "device-1".to_string(),
                    claim_token: "claim-1".to_string(),
                    now: now + Duration::seconds(10),
                    claim_ttl: Duration::seconds(30),
                    max_attempts: 3,
                },
            )
            .await
            .unwrap(),
        SchedulerTickResult::RescanRequired
    );
}
