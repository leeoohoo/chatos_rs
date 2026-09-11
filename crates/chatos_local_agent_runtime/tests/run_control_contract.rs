// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use async_trait::async_trait;
use chatos_client_storage::{
    AgentRunStateRecord, ClientStorage, ListQuery, PutRecord, RecordMetadata, RecordScope,
    SecretReference, SqliteBootstrapProfile, SqliteClientStorage, StorageEncryptionKey,
    StorageResult, StorageTransaction, TransactionRepositories,
};
use chatos_local_agent_protocol::{
    ContextStrategy, LocalAgentRun, LocalAgentRunStatus, ModelProtocol, ModelRuntimeDescriptor,
    UserInteractionAnswer,
};
use chatos_local_agent_runtime::{
    answer_run_interaction, claim_event, reduce_and_commit, request_run_control,
    AnswerRunInteraction, EventClaimRequest, EventClaimResult, ReduceAndCommitRequest,
    ReducerPolicy, RequestRunControl, RunControlAction, StepEvidence,
};
use chrono::{Duration, Utc};

fn scope() -> RecordScope {
    RecordScope {
        owner_user_id: "user-1".to_string(),
    }
}

fn run(status: LocalAgentRunStatus) -> AgentRunStateRecord {
    let now = Utc::now();
    AgentRunStateRecord {
        metadata: RecordMetadata {
            id: "run-1".to_string(),
            scope: scope(),
            origin_device_id: "device-1".to_string(),
            revision: 0,
            created_at: now,
            updated_at: now,
        },
        run: LocalAgentRun {
            run_id: "run-1".to_string(),
            profile_key: "main_chat".to_string(),
            owner_user_id: "user-1".to_string(),
            owner_entity_type: "conversation".to_string(),
            owner_entity_id: "thread-1".to_string(),
            project_id: None,
            status,
            version: 1,
            step_seq: 0,
            iteration: 0,
            retry_count: 0,
            model_config_id: "model-1".to_string(),
            model_config_revision: 1,
            model_runtime_snapshot: ModelRuntimeDescriptor {
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
            },
            context_strategy: ContextStrategy::ProviderNative,
            prompt_revision: "prompt-1".to_string(),
            capability_snapshot_ref: "capabilities-1".to_string(),
            pending_batch_id: None,
            pending_interaction: (status == LocalAgentRunStatus::Paused).then(|| {
                serde_json::json!({
                    "type": "ask_user",
                    "interaction_id": "interaction-1",
                    "question": {
                        "prompt": "Choose a visual direction",
                        "options": [],
                        "image_references": [],
                        "details": null
                    }
                })
            }),
            terminal_outcome: None,
            deadline_at: None,
            created_at: now,
            updated_at: now,
        },
    }
}

struct Seed(Option<AgentRunStateRecord>);

#[async_trait]
impl StorageTransaction for Seed {
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

async fn storage(status: LocalAgentRunStatus) -> (tempfile::TempDir, SqliteClientStorage) {
    let directory = tempfile::tempdir().unwrap();
    let storage = SqliteClientStorage::open(
        &SqliteBootstrapProfile {
            database_path: directory.path().join("client.sqlite3"),
            encryption_secret: SecretReference::new("test:control-key").unwrap(),
        },
        &StorageEncryptionKey::new([42; 32]),
    )
    .await
    .unwrap();
    storage
        .transaction(&mut Seed(Some(run(status))))
        .await
        .unwrap();
    (directory, storage)
}

fn control(action: RunControlAction) -> RequestRunControl {
    RequestRunControl {
        scope: scope(),
        run_id: "run-1".to_string(),
        action,
        origin_device_id: "device-1".to_string(),
        causation_id: "ipc-request-1".to_string(),
        now: Utc::now(),
    }
}

struct CountEvents(usize);

#[async_trait]
impl StorageTransaction for CountEvents {
    async fn execute(
        &mut self,
        repositories: &mut dyn TransactionRepositories,
    ) -> StorageResult<()> {
        self.0 = repositories
            .agent_events()
            .list(&ListQuery {
                scope: scope(),
                cursor: None,
                limit: 100,
            })
            .await?
            .records
            .len();
        Ok(())
    }
}

#[tokio::test]
async fn duplicate_control_request_is_idempotent_and_reducible() {
    let (_directory, storage) = storage(LocalAgentRunStatus::ModelRunning).await;
    let first = request_run_control(&storage, control(RunControlAction::Pause))
        .await
        .unwrap();
    let repeated = request_run_control(&storage, control(RunControlAction::Pause))
        .await
        .unwrap();
    assert_eq!(first, repeated);
    let mut count = CountEvents(0);
    storage.transaction(&mut count).await.unwrap();
    assert_eq!(count.0, 1);

    let now = Utc::now();
    let claimed = claim_event(
        &storage,
        EventClaimRequest {
            scope: scope(),
            event_id: first.event.event_id,
            device_id: "device-1".to_string(),
            claim_token: "claim-1".to_string(),
            now,
            claim_until: now + Duration::seconds(30),
            max_attempts: 3,
        },
    )
    .await
    .unwrap();
    let EventClaimResult::Acquired(claimed) = claimed else {
        panic!("control event must be claimable");
    };
    let committed = reduce_and_commit(
        &storage,
        ReduceAndCommitRequest {
            scope: scope(),
            event_id: claimed.event.event_id,
            claim_token: "claim-1".to_string(),
            origin_device_id: "device-1".to_string(),
            evidence: StepEvidence::None,
            now,
            policy: ReducerPolicy::default(),
        },
    )
    .await
    .unwrap();
    assert_eq!(committed.run_record.run.status, LocalAgentRunStatus::Paused);
}

#[tokio::test]
async fn invalid_resume_is_rejected_before_an_event_is_written() {
    let (_directory, storage) = storage(LocalAgentRunStatus::ModelReady).await;
    let error = request_run_control(&storage, control(RunControlAction::Resume))
        .await
        .unwrap_err();
    assert!(error.to_string().contains("cannot request Resume"));
    let mut count = CountEvents(0);
    storage.transaction(&mut count).await.unwrap();
    assert_eq!(count.0, 0);
}

fn answer(interaction_id: &str) -> AnswerRunInteraction {
    AnswerRunInteraction {
        scope: scope(),
        run_id: "run-1".to_string(),
        interaction_id: interaction_id.to_string(),
        answer: UserInteractionAnswer {
            text: Some("Use the editorial direction".to_string()),
            selected_option_ids: Vec::new(),
            attachments: Vec::new(),
        },
        origin_device_id: "device-1".to_string(),
        causation_id: "ipc-answer-1".to_string(),
        now: Utc::now(),
    }
}

struct CountAnswerRecords {
    messages: usize,
    outbox: usize,
    events: usize,
}

#[async_trait]
impl StorageTransaction for CountAnswerRecords {
    async fn execute(
        &mut self,
        repositories: &mut dyn TransactionRepositories,
    ) -> StorageResult<()> {
        let query = ListQuery {
            scope: scope(),
            cursor: None,
            limit: 100,
        };
        self.messages = repositories
            .agent_messages()
            .list(&query)
            .await?
            .records
            .len();
        self.outbox = repositories.sync_outbox().list(&query).await?.records.len();
        self.events = repositories
            .agent_events()
            .list(&query)
            .await?
            .records
            .len();
        Ok(())
    }
}

#[tokio::test]
async fn answer_and_resume_event_commit_atomically_and_idempotently() {
    let (_directory, storage) = storage(LocalAgentRunStatus::Paused).await;
    let first = answer_run_interaction(&storage, answer("interaction-1"))
        .await
        .unwrap();
    let repeated = answer_run_interaction(&storage, answer("interaction-1"))
        .await
        .unwrap();
    assert_eq!(first, repeated);

    let mut counts = CountAnswerRecords {
        messages: 0,
        outbox: 0,
        events: 0,
    };
    storage.transaction(&mut counts).await.unwrap();
    assert_eq!(counts.messages, 1);
    assert_eq!(counts.outbox, 1);
    assert_eq!(counts.events, 1);

    let now = Utc::now() + Duration::seconds(1);
    let claimed = claim_event(
        &storage,
        EventClaimRequest {
            scope: scope(),
            event_id: first.resume_event.event.event_id,
            device_id: "device-1".to_string(),
            claim_token: "answer-claim".to_string(),
            now,
            claim_until: now + Duration::seconds(30),
            max_attempts: 3,
        },
    )
    .await
    .unwrap();
    let EventClaimResult::Acquired(claimed) = claimed else {
        panic!("answer resume event must be claimable");
    };
    let committed = reduce_and_commit(
        &storage,
        ReduceAndCommitRequest {
            scope: scope(),
            event_id: claimed.event.event_id,
            claim_token: "answer-claim".to_string(),
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
    assert!(committed.run_record.run.pending_interaction.is_none());
}

#[tokio::test]
async fn mismatched_interaction_is_rejected_without_writes() {
    let (_directory, storage) = storage(LocalAgentRunStatus::Paused).await;
    let error = answer_run_interaction(&storage, answer("interaction-other"))
        .await
        .unwrap_err();
    assert!(error.to_string().contains("does not match"));
    let mut counts = CountAnswerRecords {
        messages: 0,
        outbox: 0,
        events: 0,
    };
    storage.transaction(&mut counts).await.unwrap();
    assert_eq!(counts.messages, 0);
    assert_eq!(counts.outbox, 0);
    assert_eq!(counts.events, 0);
}

#[tokio::test]
async fn unknown_selected_option_is_rejected_without_writes() {
    let (_directory, storage) = storage(LocalAgentRunStatus::Paused).await;
    let mut request = answer("interaction-1");
    request.answer.text = None;
    request.answer.selected_option_ids = vec!["unknown-option".to_string()];
    let error = answer_run_interaction(&storage, request).await.unwrap_err();
    assert!(error
        .to_string()
        .contains("not part of the pending question"));
    let mut counts = CountAnswerRecords {
        messages: 0,
        outbox: 0,
        events: 0,
    };
    storage.transaction(&mut counts).await.unwrap();
    assert_eq!(counts.messages, 0);
    assert_eq!(counts.outbox, 0);
    assert_eq!(counts.events, 0);
}
