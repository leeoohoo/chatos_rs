// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use async_trait::async_trait;
use chatos_client_storage::{
    AgentEventStateRecord, AgentRunStateRecord, AgentUiEventCursorQuery, ClientStorage, ListQuery,
    PutRecord, RecordMetadata, RecordQuery, RecordScope, SecretReference, SqliteBootstrapProfile,
    SqliteClientStorage, StorageEncryptionKey, StorageResult, StorageTransaction,
    ToolExecutionStateRecord, TransactionRepositories,
};
use chatos_local_agent_protocol::{
    ContextStrategy, LocalAgentEvent, LocalAgentEventStatus, LocalAgentEventType, LocalAgentRun,
    LocalAgentRunStatus, LocalAgentUiEvent, LocalAgentUiEventPayload, ModelProtocol,
    ModelRuntimeDescriptor, ModelStepCompletion, ModelStepResult, ToolEffect, ToolExecution,
    ToolExecutionStatus,
};
use chatos_local_agent_runtime::{
    claim_event, create_local_agent_run, record_model_step_completion, reduce_and_commit,
    AttemptLimitDisposition, CreateLocalAgentRunRequest, EventClaimRequest, EventClaimResult,
    RecordModelStepCompletionRequest, ReduceAndCommitRequest, ReducerPolicy, StepEvidence,
};
use chrono::{Duration, Utc};

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
                model_runtime_snapshot: model_descriptor(),
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

async fn empty_storage() -> (tempfile::TempDir, SqliteClientStorage) {
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
    (directory, storage)
}

fn create_run_request(now: chrono::DateTime<Utc>) -> CreateLocalAgentRunRequest {
    CreateLocalAgentRunRequest {
        scope: scope(),
        run_id: "created-run-1".to_string(),
        profile_key: "main_chat".to_string(),
        owner_entity_type: "conversation".to_string(),
        owner_entity_id: "conversation-created-1".to_string(),
        project_id: Some("project-1".to_string()),
        model_runtime_snapshot: model_descriptor(),
        prompt_revision: "prompt-1".to_string(),
        capability_snapshot_ref: "capabilities-1".to_string(),
        origin_device_id: "device-1".to_string(),
        causation_id: "turn-created-1".to_string(),
        deadline_at: None,
        now,
    }
}

struct CountCreatedRecords {
    runs: usize,
    events: usize,
}

#[async_trait]
impl StorageTransaction for CountCreatedRecords {
    async fn execute(
        &mut self,
        repositories: &mut dyn TransactionRepositories,
    ) -> StorageResult<()> {
        let query = ListQuery {
            scope: scope(),
            cursor: None,
            limit: 100,
        };
        self.runs = repositories.agent_runs().list(&query).await?.records.len();
        self.events = repositories
            .agent_events()
            .list(&query)
            .await?
            .records
            .len();
        Ok(())
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

struct SeedModelRunning;

#[async_trait]
impl StorageTransaction for SeedModelRunning {
    async fn execute(
        &mut self,
        repositories: &mut dyn TransactionRepositories,
    ) -> StorageResult<()> {
        let (run, _) = seed_records();
        let mut run = repositories
            .agent_runs()
            .put(PutRecord {
                record: run,
                expected_revision: None,
            })
            .await?;
        run.run.status = LocalAgentRunStatus::ModelRunning;
        run.run.version = 2;
        run.run.step_seq = 1;
        let expected_revision = run.metadata.revision;
        repositories
            .agent_runs()
            .put(PutRecord {
                record: run,
                expected_revision: Some(expected_revision),
            })
            .await?;
        Ok(())
    }
}

#[tokio::test]
async fn run_creation_commits_start_event_and_ui_snapshot_atomically_and_idempotently() {
    let (_directory, storage) = empty_storage().await;
    let now = Utc::now();
    let request = create_run_request(now);
    let first = create_local_agent_run(&storage, request.clone())
        .await
        .unwrap();
    let mut repeated_request = request;
    repeated_request.now = now + Duration::seconds(1);
    let repeated = create_local_agent_run(&storage, repeated_request)
        .await
        .unwrap();
    assert_eq!(first, repeated);
    assert_eq!(first.run_record.run.status, LocalAgentRunStatus::Queued);
    assert_eq!(first.run_record.run.version, 1);
    assert_eq!(
        first.start_event.event.event_type,
        LocalAgentEventType::RunStarted
    );
    assert_eq!(first.start_event.event.expected_version, 1);

    let mut counts = CountCreatedRecords { runs: 0, events: 0 };
    storage.transaction(&mut counts).await.unwrap();
    assert_eq!(counts.runs, 1);
    assert_eq!(counts.events, 1);
    let mut ui_events = ReadUiEvents(Vec::new());
    storage.transaction(&mut ui_events).await.unwrap();
    assert_eq!(ui_events.0.len(), 1);
    let LocalAgentUiEventPayload::RunSnapshot(snapshot) = &ui_events.0[0].event else {
        panic!("run creation must publish a Run snapshot");
    };
    assert_eq!(snapshot.run_id, "created-run-1");
}

#[tokio::test]
async fn stable_run_id_rejects_different_creation_identity_without_writes() {
    let (_directory, storage) = empty_storage().await;
    let now = Utc::now();
    create_local_agent_run(&storage, create_run_request(now))
        .await
        .unwrap();
    let mut conflicting = create_run_request(now + Duration::seconds(1));
    conflicting.project_id = Some("project-other".to_string());
    let error = create_local_agent_run(&storage, conflicting)
        .await
        .unwrap_err();
    assert!(matches!(
        error,
        chatos_client_storage::StorageError::Conflict { .. }
    ));
    let mut counts = CountCreatedRecords { runs: 0, events: 0 };
    storage.transaction(&mut counts).await.unwrap();
    assert_eq!(counts.runs, 1);
    assert_eq!(counts.events, 1);
    let mut ui_events = ReadUiEvents(Vec::new());
    storage.transaction(&mut ui_events).await.unwrap();
    assert_eq!(ui_events.0.len(), 1);
}

#[tokio::test]
async fn model_completion_is_durable_and_idempotent_before_reduction() {
    let (_directory, storage) = empty_storage().await;
    storage.transaction(&mut SeedModelRunning).await.unwrap();
    let now = Utc::now();
    let request = RecordModelStepCompletionRequest {
        scope: scope(),
        run_id: "run-1".to_string(),
        completion: ModelStepCompletion {
            result: ModelStepResult::Final(serde_json::json!({"text": "done"})),
            pending_batch_id: None,
            retry_at: None,
        },
        origin_device_id: "device-1".to_string(),
        causation_id: "model-request-1".to_string(),
        correlation_id: "conversation-1".to_string(),
        now,
    };

    let first = record_model_step_completion(&storage, request.clone())
        .await
        .unwrap();
    let repeated = record_model_step_completion(&storage, request)
        .await
        .unwrap();

    assert_eq!(first.event.event_id, "run-1:2:model_step_completed:0");
    assert_eq!(first, repeated);
    assert_eq!(first.event.status, LocalAgentEventStatus::Pending);
    assert_eq!(first.event.expected_version, 2);
    let decoded: ModelStepCompletion = serde_json::from_value(first.event.bounded_payload).unwrap();
    assert!(matches!(decoded.result, ModelStepResult::Final(_)));
}

#[tokio::test]
async fn ask_user_reduction_publishes_a_visual_interaction_with_the_run_snapshot() {
    let (_directory, storage) = empty_storage().await;
    storage.transaction(&mut SeedModelRunning).await.unwrap();
    let now = Utc::now();
    let question = serde_json::json!({
        "prompt": "Choose the homepage art direction",
        "options": [{
            "option_id": "editorial",
            "label": "Editorial",
            "description": "Typography-led and spacious"
        }],
        "image_references": ["homepage-preview-1"],
        "details": {"annotation_id": "annotation-1"}
    });
    let completion = record_model_step_completion(
        &storage,
        RecordModelStepCompletionRequest {
            scope: scope(),
            run_id: "run-1".to_string(),
            completion: ModelStepCompletion {
                result: ModelStepResult::AskUser(question.clone()),
                pending_batch_id: None,
                retry_at: None,
            },
            origin_device_id: "device-1".to_string(),
            causation_id: "model-request-1".to_string(),
            correlation_id: "conversation-1".to_string(),
            now,
        },
    )
    .await
    .unwrap();
    let claimed = claim_event(
        &storage,
        EventClaimRequest {
            scope: scope(),
            event_id: completion.event.event_id,
            device_id: "device-1".to_string(),
            claim_token: "ask-user-claim".to_string(),
            now,
            claim_until: now + Duration::seconds(30),
            max_attempts: 3,
        },
    )
    .await
    .unwrap();
    let EventClaimResult::Acquired(claimed) = claimed else {
        panic!("Ask User completion must be claimable");
    };
    let committed = reduce_and_commit(
        &storage,
        ReduceAndCommitRequest {
            scope: scope(),
            event_id: claimed.event.event_id,
            claim_token: "ask-user-claim".to_string(),
            origin_device_id: "device-1".to_string(),
            evidence: StepEvidence::Model {
                result: ModelStepResult::AskUser(question),
                pending_batch_id: None,
                retry_at: None,
            },
            now,
            policy: ReducerPolicy::default(),
        },
    )
    .await
    .unwrap();
    assert_eq!(committed.run_record.run.status, LocalAgentRunStatus::Paused);

    let mut ui_events = ReadUiEvents(Vec::new());
    storage.transaction(&mut ui_events).await.unwrap();
    assert_eq!(ui_events.0.len(), 2);
    assert!(matches!(
        ui_events.0[0].event,
        LocalAgentUiEventPayload::RunSnapshot(_)
    ));
    let LocalAgentUiEventPayload::UserInteraction(interaction) = &ui_events.0[1].event else {
        panic!("second UI event must be the Ask User interaction");
    };
    assert_eq!(interaction.run_id, "run-1");
    assert_eq!(interaction.prompt, "Choose the homepage art direction");
    assert_eq!(interaction.options[0].option_id, "editorial");
    assert_eq!(interaction.image_references, ["homepage-preview-1"]);
    assert_eq!(
        interaction.details.as_ref().unwrap()["annotation_id"],
        "annotation-1"
    );
    assert_eq!(
        committed
            .run_record
            .run
            .pending_interaction
            .as_ref()
            .unwrap()["interaction_id"],
        interaction.interaction_id
    );
    assert_eq!(ui_events.0[1].event_seq, ui_events.0[0].event_seq + 1);
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
            max_attempts: 3,
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
    let mut ui_events = ReadUiEvents(Vec::new());
    storage.transaction(&mut ui_events).await.unwrap();
    assert_eq!(ui_events.0.len(), 1);
    let LocalAgentUiEventPayload::RunSnapshot(snapshot) = &ui_events.0[0].event else {
        panic!("reduction must atomically publish a Run snapshot");
    };
    assert_eq!(snapshot.status, LocalAgentRunStatus::ModelReady);
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
                max_attempts: 3,
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
            max_attempts: 3,
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

struct ReadAttemptLimitState {
    run: Option<AgentRunStateRecord>,
    event: Option<AgentEventStateRecord>,
    tool: Option<ToolExecutionStateRecord>,
    terminal_event: Option<AgentEventStateRecord>,
}

#[async_trait]
impl StorageTransaction for ReadAttemptLimitState {
    async fn execute(
        &mut self,
        repositories: &mut dyn TransactionRepositories,
    ) -> StorageResult<()> {
        self.run = repositories
            .agent_runs()
            .get(&RecordQuery {
                scope: scope(),
                id: "run-1".to_string(),
            })
            .await?;
        self.event = repositories
            .agent_events()
            .get(&RecordQuery {
                scope: scope(),
                id: "event-1".to_string(),
            })
            .await?;
        self.tool = repositories
            .tool_executions()
            .get(&RecordQuery {
                scope: scope(),
                id: "invocation-1".to_string(),
            })
            .await?;
        self.terminal_event = repositories
            .agent_events()
            .get(&RecordQuery {
                scope: scope(),
                id: "run-1:2:run_terminal:0".to_string(),
            })
            .await?;
        Ok(())
    }
}

fn empty_attempt_limit_state() -> ReadAttemptLimitState {
    ReadAttemptLimitState {
        run: None,
        event: None,
        tool: None,
        terminal_event: None,
    }
}

#[tokio::test]
async fn exhausted_ordinary_event_fails_run_and_emits_terminal_event_atomically() {
    let (_directory, storage) = storage().await;
    let now = Utc::now();
    assert!(matches!(
        claim_event(
            &storage,
            EventClaimRequest {
                scope: scope(),
                event_id: "event-1".to_string(),
                device_id: "device-1".to_string(),
                claim_token: "claim-1".to_string(),
                now,
                claim_until: now + Duration::seconds(30),
                max_attempts: 1,
            },
        )
        .await
        .unwrap(),
        EventClaimResult::Acquired(_)
    ));

    let result = claim_event(
        &storage,
        EventClaimRequest {
            scope: scope(),
            event_id: "event-1".to_string(),
            device_id: "device-1".to_string(),
            claim_token: "claim-2".to_string(),
            now: now + Duration::seconds(31),
            claim_until: now + Duration::seconds(61),
            max_attempts: 1,
        },
    )
    .await
    .unwrap();
    assert_eq!(
        result,
        EventClaimResult::AttemptsExhausted {
            disposition: AttemptLimitDisposition::RunFailed,
        }
    );

    let mut state = empty_attempt_limit_state();
    storage.transaction(&mut state).await.unwrap();
    assert_eq!(state.run.unwrap().run.status, LocalAgentRunStatus::Failed);
    assert_eq!(
        state.event.unwrap().event.status,
        LocalAgentEventStatus::Failed
    );
    assert_eq!(
        state.terminal_event.unwrap().event.event_type,
        LocalAgentEventType::RunTerminal
    );
}

struct SeedUnknownIrreversibleTool {
    now: chrono::DateTime<Utc>,
}

#[async_trait]
impl StorageTransaction for SeedUnknownIrreversibleTool {
    async fn execute(
        &mut self,
        repositories: &mut dyn TransactionRepositories,
    ) -> StorageResult<()> {
        let (mut run, mut event) = seed_records();
        run.run.created_at = self.now;
        run.run.updated_at = self.now;
        run.run.status = LocalAgentRunStatus::WaitingToolResult;
        run.run.pending_batch_id = Some("batch-1".to_string());
        event.event.event_type = LocalAgentEventType::ToolBatchRequested;
        event.event.status = LocalAgentEventStatus::Claimed;
        event.event.attempt_count = 1;
        event.event.claimed_by_device_id = Some("device-old".to_string());
        event.event.claim_token = Some("claim-old".to_string());
        event.event.claim_until = Some(self.now - Duration::seconds(1));
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
        repositories
            .tool_executions()
            .put(PutRecord {
                record: ToolExecutionStateRecord {
                    metadata: metadata("invocation-1", self.now),
                    execution: ToolExecution {
                        invocation_id: "invocation-1".to_string(),
                        run_id: "run-1".to_string(),
                        batch_id: "batch-1".to_string(),
                        tool_call_id: "call-1".to_string(),
                        tool_name: "write_file".to_string(),
                        effect: ToolEffect::Write,
                        arguments_digest: "sha256:0000000000000000000000000000000000000000000000000000000000000000".to_string(),
                        status: ToolExecutionStatus::Started,
                        bounded_result: None,
                        started_at: Some(self.now - Duration::seconds(10)),
                        completed_at: None,
                    },
                },
                expected_revision: None,
            })
            .await?;
        Ok(())
    }
}

#[tokio::test]
async fn exhausted_event_never_replays_an_unknown_irreversible_tool() {
    let now = Utc::now();
    let (_directory, storage) = empty_storage().await;
    storage
        .transaction(&mut SeedUnknownIrreversibleTool { now })
        .await
        .unwrap();

    let result = claim_event(
        &storage,
        EventClaimRequest {
            scope: scope(),
            event_id: "event-1".to_string(),
            device_id: "device-1".to_string(),
            claim_token: "claim-new".to_string(),
            now,
            claim_until: now + Duration::seconds(30),
            max_attempts: 1,
        },
    )
    .await
    .unwrap();
    assert_eq!(
        result,
        EventClaimResult::AttemptsExhausted {
            disposition: AttemptLimitDisposition::NeedsReview,
        }
    );

    let mut state = empty_attempt_limit_state();
    storage.transaction(&mut state).await.unwrap();
    let run = state.run.unwrap().run;
    assert_eq!(run.status, LocalAgentRunStatus::NeedsReview);
    assert!(run.pending_interaction.is_some());
    assert_eq!(
        state.tool.unwrap().execution.status,
        ToolExecutionStatus::OutcomeUnknown
    );
    assert!(state.terminal_event.is_none());
}
