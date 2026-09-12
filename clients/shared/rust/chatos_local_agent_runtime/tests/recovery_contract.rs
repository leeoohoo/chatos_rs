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
    LocalAgentRunStatus, ModelProtocol, ModelRuntimeDescriptor,
};
use chatos_local_agent_runtime::{scan_recoverable_work, RecoveryIssue};
use chrono::{DateTime, Duration, Utc};

fn model_descriptor() -> ModelRuntimeDescriptor {
    ModelRuntimeDescriptor {
        model_config_id: "model-1".to_string(),
        revision: 1,
        provider: "openai".to_string(),
        model: "gpt-5".to_string(),
        protocol: ModelProtocol::Responses,
        context_window_tokens: 400_000,
        maximum_output_tokens: 32_000,
        context_strategy: ContextStrategy::ProviderNative,
        supports_streaming: true,
        supports_native_compaction: true,
        supports_input_token_count: true,
    }
}

fn scope() -> RecordScope {
    RecordScope {
        owner_user_id: "user-1".to_string(),
    }
}

fn metadata(id: &str, now: DateTime<Utc>) -> RecordMetadata {
    RecordMetadata {
        id: id.to_string(),
        scope: scope(),
        origin_device_id: "device-1".to_string(),
        revision: 0,
        created_at: now,
        updated_at: now,
    }
}

fn run(id: &str, status: LocalAgentRunStatus, now: DateTime<Utc>) -> AgentRunStateRecord {
    AgentRunStateRecord {
        metadata: metadata(id, now),
        run: LocalAgentRun {
            run_id: id.to_string(),
            profile_key: "task_runner".to_string(),
            owner_user_id: "user-1".to_string(),
            owner_entity_type: "task".to_string(),
            owner_entity_id: format!("task-{id}"),
            project_id: Some("project-1".to_string()),
            status,
            version: 1,
            step_seq: 0,
            iteration: 0,
            retry_count: 0,
            model_config_id: "model-1".to_string(),
            model_config_revision: 1,
            model_runtime_snapshot: model_descriptor(),
            context_strategy: ContextStrategy::ProviderNative,
            prompt_revision: "prompt-1".to_string(),
            capability_snapshot_ref: "capabilities-1".to_string(),
            pending_batch_id: None,
            pending_interaction: None,
            terminal_outcome: status
                .is_terminal()
                .then(|| serde_json::json!({"result": "done"})),
            deadline_at: None,
            created_at: now,
            updated_at: now,
        },
    }
}

fn event(
    id: &str,
    run_id: &str,
    status: LocalAgentEventStatus,
    available_at: DateTime<Utc>,
    claim_until: Option<DateTime<Utc>>,
) -> AgentEventStateRecord {
    let claimed = status == LocalAgentEventStatus::Claimed;
    AgentEventStateRecord {
        metadata: metadata(id, available_at),
        event: LocalAgentEvent {
            event_id: id.to_string(),
            run_id: run_id.to_string(),
            event_type: LocalAgentEventType::RunStarted,
            expected_version: 1,
            available_at,
            status,
            attempt_count: u32::from(claimed),
            claimed_by_device_id: claimed.then(|| "device-old".to_string()),
            claim_token: claimed.then(|| format!("claim-{id}")),
            claim_until,
            causation_id: "turn-1".to_string(),
            correlation_id: "task-1".to_string(),
            bounded_payload: serde_json::Value::Null,
            last_error: None,
        },
    }
}

struct SeedRecoveryState {
    now: DateTime<Utc>,
}

#[async_trait]
impl StorageTransaction for SeedRecoveryState {
    async fn execute(
        &mut self,
        repositories: &mut dyn TransactionRepositories,
    ) -> StorageResult<()> {
        for record in [
            run("run-due", LocalAgentRunStatus::Queued, self.now),
            run("run-expired", LocalAgentRunStatus::Queued, self.now),
            run("run-future", LocalAgentRunStatus::RetryScheduled, self.now),
            run("run-live-claim", LocalAgentRunStatus::Queued, self.now),
            run("run-stranded", LocalAgentRunStatus::ModelRunning, self.now),
            run("run-paused", LocalAgentRunStatus::Paused, self.now),
            run("run-terminal", LocalAgentRunStatus::Succeeded, self.now),
        ] {
            repositories
                .agent_runs()
                .put(PutRecord {
                    record,
                    expected_revision: None,
                })
                .await?;
        }
        for record in [
            event(
                "event-due",
                "run-due",
                LocalAgentEventStatus::Pending,
                self.now,
                None,
            ),
            event(
                "event-expired",
                "run-expired",
                LocalAgentEventStatus::Claimed,
                self.now - Duration::minutes(2),
                Some(self.now - Duration::minutes(1)),
            ),
            event(
                "event-future",
                "run-future",
                LocalAgentEventStatus::Pending,
                self.now + Duration::minutes(5),
                None,
            ),
            event(
                "event-live-claim",
                "run-live-claim",
                LocalAgentEventStatus::Claimed,
                self.now - Duration::minutes(1),
                Some(self.now + Duration::minutes(3)),
            ),
            event(
                "event-terminal",
                "run-terminal",
                LocalAgentEventStatus::Pending,
                self.now,
                None,
            ),
            event(
                "event-orphan",
                "run-missing",
                LocalAgentEventStatus::Pending,
                self.now,
                None,
            ),
        ] {
            repositories
                .agent_events()
                .put(PutRecord {
                    record,
                    expected_revision: None,
                })
                .await?;
        }
        Ok(())
    }
}

#[tokio::test]
async fn startup_scan_recovers_only_due_or_expired_work() {
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
    storage
        .transaction(&mut SeedRecoveryState { now })
        .await
        .unwrap();

    let plan = scan_recoverable_work(&storage, scope(), now).await.unwrap();

    assert_eq!(plan.active_runs.len(), 6);
    assert_eq!(
        plan.ready_events
            .iter()
            .map(|record| record.event.event_id.as_str())
            .collect::<Vec<_>>(),
        vec!["event-expired", "event-due"]
    );
    assert_eq!(plan.next_wake_at, Some(now + Duration::minutes(3)));
    assert!(plan.issues.contains(&RecoveryIssue::StrandedRun {
        run_id: "run-stranded".to_string(),
        status: LocalAgentRunStatus::ModelRunning,
    }));
    assert!(plan.issues.contains(&RecoveryIssue::OrphanedEvent {
        event_id: "event-orphan".to_string(),
        run_id: "run-missing".to_string(),
    }));
    assert!(plan.issues.contains(&RecoveryIssue::EventForTerminalRun {
        event_id: "event-terminal".to_string(),
        run_id: "run-terminal".to_string(),
    }));
    assert!(!plan.issues.iter().any(|issue| matches!(
        issue,
        RecoveryIssue::StrandedRun { run_id, .. } if run_id == "run-paused"
    )));
}
