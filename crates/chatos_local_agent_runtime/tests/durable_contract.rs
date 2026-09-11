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
use chatos_local_agent_runtime::{
    claim_event, reduce_and_commit, EventClaimRequest, EventClaimResult, ReduceAndCommitRequest,
    ReducerPolicy, StepEvidence,
};
use chrono::{Duration, Utc};

fn scope() -> RecordScope {
    RecordScope {
        owner_user_id: "user-1".to_string(),
    }
}

fn metadata(id: &str, now: chrono::DateTime<Utc>) -> RecordMetadata {
    RecordMetadata {
        id: id.to_string(),
        scope: scope(),
        origin_device_id: "device-1".to_string(),
        revision: 0,
        created_at: now,
        updated_at: now,
    }
}

fn seed_records() -> (AgentRunStateRecord, AgentEventStateRecord) {
    let now = Utc::now();
    (
        AgentRunStateRecord {
            metadata: metadata("run-1", now),
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
                created_at: now,
                updated_at: now,
            },
        },
        AgentEventStateRecord {
            metadata: metadata("event-1", now),
            event: LocalAgentEvent {
                event_id: "event-1".to_string(),
                run_id: "run-1".to_string(),
                event_type: LocalAgentEventType::RunStarted,
                expected_version: 1,
                available_at: now,
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
    )
}

struct Seed;

#[async_trait]
impl StorageTransaction for Seed {
    async fn execute(
        &mut self,
        repositories: &mut dyn TransactionRepositories,
    ) -> StorageResult<()> {
        let (run, event) = seed_records();
        repositories
            .agent_runs()
            .put(PutRecord {
                record: run,
                expected_revision: None,
            })
            .await?;
        repositories
            .agent_events()
            .put(PutRecord {
                record: event,
                expected_revision: None,
            })
            .await?;
        Ok(())
    }
}

async fn storage() -> (tempfile::TempDir, SqliteClientStorage) {
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
    storage.transaction(&mut Seed).await.unwrap();
    (directory, storage)
}

#[tokio::test]
async fn claim_and_reduction_commit_are_durable_and_idempotent() {
    let (_directory, storage) = storage().await;
    let now = Utc::now();
    let claimed = claim_event(
        &storage,
        EventClaimRequest {
            scope: scope(),
            event_id: "event-1".to_string(),
            device_id: "device-1".to_string(),
            claim_token: "claim-1".to_string(),
            now,
            claim_until: now + Duration::seconds(30),
        },
    )
    .await
    .unwrap();
    let EventClaimResult::Acquired(claimed) = claimed else {
        panic!("event was not claimed");
    };
    assert_eq!(claimed.event.attempt_count, 1);
    assert_eq!(claimed.event.claim_token.as_deref(), Some("claim-1"));

    let committed = reduce_and_commit(
        &storage,
        ReduceAndCommitRequest {
            scope: scope(),
            event_id: "event-1".to_string(),
            claim_token: "claim-1".to_string(),
            origin_device_id: "device-1".to_string(),
            evidence: StepEvidence::None,
            now,
            policy: ReducerPolicy::default(),
        },
    )
    .await
    .unwrap();
    assert_eq!(
        committed.run_record.run.status,
        LocalAgentRunStatus::ModelReady
    );
    assert_eq!(committed.run_record.run.version, 2);
    assert_eq!(
        committed.applied_event.event.status,
        LocalAgentEventStatus::Applied
    );
    assert_eq!(committed.emitted_events.len(), 1);
    assert_eq!(
        committed.emitted_events[0].event.event_type,
        LocalAgentEventType::ModelStepRequested
    );
    assert_eq!(
        claim_event(
            &storage,
            EventClaimRequest {
                scope: scope(),
                event_id: "event-1".to_string(),
                device_id: "device-1".to_string(),
                claim_token: "claim-2".to_string(),
                now,
                claim_until: now + Duration::seconds(30),
            },
        )
        .await
        .unwrap(),
        EventClaimResult::AlreadyFinished
    );
}

#[tokio::test]
async fn a_stale_claim_token_cannot_reduce_the_event() {
    let (_directory, storage) = storage().await;
    let now = Utc::now();
    claim_event(
        &storage,
        EventClaimRequest {
            scope: scope(),
            event_id: "event-1".to_string(),
            device_id: "device-1".to_string(),
            claim_token: "active-claim".to_string(),
            now,
            claim_until: now + Duration::seconds(30),
        },
    )
    .await
    .unwrap();
    let result = reduce_and_commit(
        &storage,
        ReduceAndCommitRequest {
            scope: scope(),
            event_id: "event-1".to_string(),
            claim_token: "stale-claim".to_string(),
            origin_device_id: "device-1".to_string(),
            evidence: StepEvidence::None,
            now,
            policy: ReducerPolicy::default(),
        },
    )
    .await;
    assert!(matches!(
        result,
        Err(chatos_client_storage::StorageError::Conflict { .. })
    ));

    struct ReadEvent(Option<AgentEventStateRecord>);
    #[async_trait]
    impl StorageTransaction for ReadEvent {
        async fn execute(
            &mut self,
            repositories: &mut dyn TransactionRepositories,
        ) -> StorageResult<()> {
            self.0 = repositories
                .agent_events()
                .get(&RecordQuery {
                    scope: scope(),
                    id: "event-1".to_string(),
                })
                .await?;
            Ok(())
        }
    }
    let mut read = ReadEvent(None);
    storage.transaction(&mut read).await.unwrap();
    assert_eq!(read.0.unwrap().event.status, LocalAgentEventStatus::Claimed);
}
