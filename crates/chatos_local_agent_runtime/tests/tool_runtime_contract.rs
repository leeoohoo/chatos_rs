// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::Arc;

use async_trait::async_trait;
use chatos_client_storage::{
    AgentEventStateRecord, AgentMessageStateRecord, AgentRunStateRecord, AgentUiEventCursorQuery,
    ClientStorage, ListQuery, PutRecord, RecordMetadata, RecordScope, SecretReference,
    SqliteBootstrapProfile, SqliteClientStorage, StorageEncryptionKey, StorageResult,
    StorageTransaction, TransactionRepositories,
};
use chatos_local_agent_protocol::{
    AgentMessage, AgentMessageRole, ContextStrategy, LocalAgentEvent, LocalAgentEventStatus,
    LocalAgentEventType, LocalAgentRun, LocalAgentRunStatus, LocalAgentUiEvent,
    LocalAgentUiEventPayload, MemorySyncStatus, MessageMode, ModelProtocol, ModelRuntimeDescriptor,
    ToolApprovalDecision, ToolExecutionStatus,
};
use chatos_local_agent_runtime::{
    begin_tool_execution, complete_tool_execution, decide_tool_approval,
    defer_tool_batch_for_approval, inspect_tool_batch, mark_tool_outcome_unknown,
    prepare_tool_batch, BeginToolExecutionRequest, BeginToolExecutionResult,
    CompleteToolExecutionRequest, DecideToolApprovalRequest, DeferToolBatchForApprovalRequest,
    MarkToolOutcomeUnknownRequest, PrepareToolBatchRequest,
};
use chrono::{Duration, Utc};
use serde_json::json;
use sha2::{Digest, Sha256};

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

struct Seed {
    now: chrono::DateTime<Utc>,
    project_id: &'static str,
}

#[async_trait]
impl StorageTransaction for Seed {
    async fn execute(
        &mut self,
        repositories: &mut dyn TransactionRepositories,
    ) -> StorageResult<()> {
        let descriptor = ModelRuntimeDescriptor {
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
        };
        repositories
            .agent_runs()
            .put(PutRecord {
                record: AgentRunStateRecord {
                    metadata: metadata("run-1", self.now),
                    run: LocalAgentRun {
                        run_id: "run-1".to_string(),
                        profile_key: "task_runner".to_string(),
                        owner_user_id: "user-1".to_string(),
                        owner_entity_type: "task".to_string(),
                        owner_entity_id: "task-1".to_string(),
                        project_id: Some("project-1".to_string()),
                        status: LocalAgentRunStatus::WaitingToolResult,
                        version: 1,
                        step_seq: 1,
                        iteration: 1,
                        retry_count: 0,
                        model_config_id: "model-1".to_string(),
                        model_config_revision: 1,
                        model_runtime_snapshot: descriptor,
                        context_strategy: ContextStrategy::ProviderNative,
                        prompt_revision: "prompt-1".to_string(),
                        capability_snapshot_ref: "capabilities-1".to_string(),
                        pending_batch_id: Some("batch-1".to_string()),
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
                    metadata: metadata("event-1", self.now),
                    event: LocalAgentEvent {
                        event_id: "event-1".to_string(),
                        run_id: "run-1".to_string(),
                        event_type: LocalAgentEventType::ToolBatchRequested,
                        expected_version: 1,
                        available_at: self.now,
                        status: LocalAgentEventStatus::Claimed,
                        attempt_count: 1,
                        claimed_by_device_id: Some("device-1".to_string()),
                        claim_token: Some("claim-1".to_string()),
                        claim_until: Some(self.now + Duration::minutes(5)),
                        causation_id: "model-step-1".to_string(),
                        correlation_id: "task-1".to_string(),
                        bounded_payload: json!({
                            "project_id": self.project_id,
                            "capability_snapshot_ref": "capabilities-1",
                            "calls": [
                                {
                                    "call_id": "call-read",
                                    "name": "read_file",
                                    "effect": "read",
                                    "arguments": {"path": "src/lib.rs", "options": {"b": 2, "a": 1}}
                                },
                                {
                                    "call_id": "call-write",
                                    "name": "write_file",
                                    "effect": "write",
                                    "arguments": {"path": "src/lib.rs", "content": "updated"}
                                },
                                {
                                    "call_id": "call-idempotent-write",
                                    "name": "create_local_task",
                                    "effect": "idempotent_write",
                                    "arguments": {
                                        "objective": "Implement the reviewed design",
                                        "acceptance_criteria": ["The visual contract is verified"]
                                    }
                                }
                            ]
                        }),
                        last_error: None,
                    },
                },
                expected_revision: None,
            })
            .await?;
        Ok(())
    }
}

async fn storage(project_id: &'static str) -> (tempfile::TempDir, Arc<SqliteClientStorage>) {
    let directory = tempfile::tempdir().unwrap();
    let storage = Arc::new(
        SqliteClientStorage::open(
            &SqliteBootstrapProfile {
                database_path: directory.path().join("client.sqlite3"),
                encryption_secret: SecretReference::new("test:sqlite-key").unwrap(),
            },
            &StorageEncryptionKey::new([42; 32]),
        )
        .await
        .unwrap(),
    );
    storage
        .transaction(&mut Seed {
            now: Utc::now(),
            project_id,
        })
        .await
        .unwrap();
    (directory, storage)
}

fn prepare_request() -> PrepareToolBatchRequest {
    PrepareToolBatchRequest {
        scope: scope(),
        event_id: "event-1".to_string(),
        claim_token: "claim-1".to_string(),
        now: Utc::now(),
    }
}

async fn approve(storage: &SqliteClientStorage, invocation_id: String) {
    decide_tool_approval(
        storage,
        DecideToolApprovalRequest {
            scope: scope(),
            invocation_id,
            decision: ToolApprovalDecision::Approve,
            reason: Some("approved by the local user".to_string()),
            now: Utc::now(),
        },
    )
    .await
    .unwrap();
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

struct ReadEvent(Option<AgentEventStateRecord>);

#[async_trait]
impl StorageTransaction for ReadEvent {
    async fn execute(
        &mut self,
        repositories: &mut dyn TransactionRepositories,
    ) -> StorageResult<()> {
        self.0 = repositories
            .agent_events()
            .get(&chatos_client_storage::RecordQuery {
                scope: scope(),
                id: "event-1".to_string(),
            })
            .await?;
        Ok(())
    }
}

async fn read_event(storage: &SqliteClientStorage) -> AgentEventStateRecord {
    let mut operation = ReadEvent(None);
    storage.transaction(&mut operation).await.unwrap();
    operation.0.unwrap()
}

struct ReadSemanticState {
    messages: Vec<AgentMessage>,
    outbox_count: usize,
}

#[async_trait]
impl StorageTransaction for ReadSemanticState {
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
            .into_iter()
            .map(|record| record.message)
            .collect();
        self.outbox_count = repositories.sync_outbox().list(&query).await?.records.len();
        Ok(())
    }
}

async fn read_semantic_state(storage: &SqliteClientStorage) -> ReadSemanticState {
    let mut operation = ReadSemanticState {
        messages: Vec::new(),
        outbox_count: 0,
    };
    storage.transaction(&mut operation).await.unwrap();
    operation
}

fn tool_result_message_id(invocation_id: &str) -> String {
    let mut hasher = Sha256::new();
    for value in ["user-1", invocation_id] {
        hasher.update((value.len() as u64).to_be_bytes());
        hasher.update(value.as_bytes());
    }
    format!("tool-result:{:x}", hasher.finalize())
}

#[tokio::test]
async fn batch_and_arguments_are_frozen_idempotently_before_execution() {
    let (_directory, storage) = storage("project-1").await;
    let first = prepare_tool_batch(storage.as_ref(), prepare_request())
        .await
        .unwrap();
    let repeated = prepare_tool_batch(storage.as_ref(), prepare_request())
        .await
        .unwrap();

    assert_eq!(first, repeated);
    assert_eq!(first.project_id.as_deref(), Some("project-1"));
    assert_eq!(first.calls.len(), 3);
    assert_eq!(first.calls[0].tool_name, "read_file");
    assert!(first.calls[0].invocation_id.starts_with("tool:"));
    let events = read_ui_events(storage.as_ref()).await;
    assert_eq!(
        events.len(),
        3,
        "idempotent preparation emits no duplicates"
    );
    let statuses = events
        .iter()
        .filter_map(|event| match &event.event {
            LocalAgentUiEventPayload::ToolSnapshot(snapshot) => Some(snapshot.status),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        statuses,
        vec![
            ToolExecutionStatus::Requested,
            ToolExecutionStatus::AwaitingApproval,
            ToolExecutionStatus::AwaitingApproval,
        ]
    );
}

#[tokio::test]
async fn claimed_batch_stays_dormant_until_every_approval_is_durable() {
    let (_directory, storage) = storage("project-1").await;
    let batch = prepare_tool_batch(storage.as_ref(), prepare_request())
        .await
        .unwrap();
    let deferred_at = Utc::now();
    defer_tool_batch_for_approval(
        storage.as_ref(),
        DeferToolBatchForApprovalRequest {
            scope: scope(),
            event_id: "event-1".to_string(),
            claim_token: "claim-1".to_string(),
            batch: batch.clone(),
            now: deferred_at,
        },
    )
    .await
    .unwrap();
    let deferred = read_event(storage.as_ref()).await;
    assert_eq!(deferred.event.status, LocalAgentEventStatus::Pending);
    assert!(deferred.event.claim_token.is_none());
    assert!(deferred.event.available_at > deferred_at + Duration::days(3_000));

    decide_tool_approval(
        storage.as_ref(),
        DecideToolApprovalRequest {
            scope: scope(),
            invocation_id: batch.calls[1].invocation_id.clone(),
            decision: ToolApprovalDecision::Approve,
            reason: None,
            now: deferred_at + Duration::seconds(1),
        },
    )
    .await
    .unwrap();
    assert_eq!(
        read_event(storage.as_ref()).await.event.available_at,
        deferred.event.available_at,
        "one unresolved approval must keep the batch dormant"
    );

    let resumed_at = deferred_at + Duration::seconds(2);
    decide_tool_approval(
        storage.as_ref(),
        DecideToolApprovalRequest {
            scope: scope(),
            invocation_id: batch.calls[2].invocation_id.clone(),
            decision: ToolApprovalDecision::Reject,
            reason: Some("do not create another task".to_string()),
            now: resumed_at,
        },
    )
    .await
    .unwrap();
    assert_eq!(
        read_event(storage.as_ref()).await.event.available_at,
        resumed_at
    );
}

#[tokio::test]
async fn read_execution_can_resume_and_completion_is_idempotent() {
    let (_directory, storage) = storage("project-1").await;
    let batch = prepare_tool_batch(storage.as_ref(), prepare_request())
        .await
        .unwrap();
    let invocation_id = batch.calls[0].invocation_id.clone();
    let begin = BeginToolExecutionRequest {
        scope: scope(),
        invocation_id: invocation_id.clone(),
        now: Utc::now(),
    };
    assert!(matches!(
        begin_tool_execution(storage.as_ref(), begin.clone())
            .await
            .unwrap(),
        BeginToolExecutionResult::Execute(_)
    ));
    assert!(matches!(
        begin_tool_execution(storage.as_ref(), begin).await.unwrap(),
        BeginToolExecutionResult::Execute(_)
    ));

    let complete = CompleteToolExecutionRequest {
        scope: scope(),
        invocation_id,
        status: ToolExecutionStatus::Succeeded,
        bounded_result: json!({"content": "ok"}),
        now: Utc::now(),
    };
    let first = complete_tool_execution(storage.as_ref(), complete.clone())
        .await
        .unwrap();
    let repeated = complete_tool_execution(storage.as_ref(), complete)
        .await
        .unwrap();
    assert_eq!(first, repeated);

    let state = inspect_tool_batch(storage.as_ref(), scope(), &batch)
        .await
        .unwrap();
    assert!(!state.all_completed);
    assert!(!state.outcome_unknown);
    let events = read_ui_events(storage.as_ref()).await;
    let tool_events = events
        .iter()
        .filter_map(|event| match &event.event {
            LocalAgentUiEventPayload::ToolSnapshot(snapshot) => Some(snapshot),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        tool_events.len(),
        5,
        "only real tool transitions are published"
    );
    let snapshot = tool_events.last().unwrap();
    assert_eq!(snapshot.status, ToolExecutionStatus::Succeeded);

    let semantic = read_semantic_state(storage.as_ref()).await;
    assert_eq!(semantic.messages.len(), 1);
    assert_eq!(semantic.outbox_count, 1);
    let message = &semantic.messages[0];
    assert_eq!(message.role, AgentMessageRole::Tool);
    assert_eq!(message.thread_id, "task-1");
    assert_eq!(message.tool_call_id.as_deref(), Some("call-read"));
    assert_eq!(message.content.as_deref(), Some("{\"content\":\"ok\"}"));
    assert_eq!(
        message.structured_payload.as_ref().unwrap()["status"],
        "succeeded"
    );
}

#[tokio::test]
async fn irreversible_started_execution_is_never_replayed_after_reentry() {
    let (_directory, storage) = storage("project-1").await;
    let batch = prepare_tool_batch(storage.as_ref(), prepare_request())
        .await
        .unwrap();
    let invocation_id = batch.calls[1].invocation_id.clone();
    assert!(matches!(
        begin_tool_execution(
            storage.as_ref(),
            BeginToolExecutionRequest {
                scope: scope(),
                invocation_id: invocation_id.clone(),
                now: Utc::now(),
            },
        )
        .await
        .unwrap(),
        BeginToolExecutionResult::AwaitingApproval(_)
    ));
    approve(storage.as_ref(), invocation_id.clone()).await;
    let begin = BeginToolExecutionRequest {
        scope: scope(),
        invocation_id,
        now: Utc::now(),
    };
    assert!(matches!(
        begin_tool_execution(storage.as_ref(), begin.clone())
            .await
            .unwrap(),
        BeginToolExecutionResult::Execute(_)
    ));
    let result = begin_tool_execution(storage.as_ref(), begin).await.unwrap();
    let BeginToolExecutionResult::NeedsReview(record) = result else {
        panic!("irreversible execution must not replay");
    };
    assert_eq!(record.execution.status, ToolExecutionStatus::OutcomeUnknown);
    let semantic = read_semantic_state(storage.as_ref()).await;
    assert_eq!(semantic.messages.len(), 1);
    assert_eq!(semantic.outbox_count, 1);
    assert_eq!(semantic.messages[0].role, AgentMessageRole::Tool);
    assert_eq!(
        semantic.messages[0].structured_payload.as_ref().unwrap()["status"],
        "outcome_unknown"
    );
}

#[tokio::test]
async fn idempotent_write_started_execution_is_replayed_after_reentry() {
    let (_directory, storage) = storage("project-1").await;
    let batch = prepare_tool_batch(storage.as_ref(), prepare_request())
        .await
        .unwrap();
    let invocation_id = batch.calls[2].invocation_id.clone();
    approve(storage.as_ref(), invocation_id.clone()).await;
    let begin = BeginToolExecutionRequest {
        scope: scope(),
        invocation_id,
        now: Utc::now(),
    };
    assert!(matches!(
        begin_tool_execution(storage.as_ref(), begin.clone())
            .await
            .unwrap(),
        BeginToolExecutionResult::Execute(_)
    ));
    let repeated = begin_tool_execution(storage.as_ref(), begin).await.unwrap();
    let BeginToolExecutionResult::Execute(record) = repeated else {
        panic!("idempotent write must be safe to replay after a crash");
    };
    assert_eq!(record.execution.status, ToolExecutionStatus::Started);

    let error = mark_tool_outcome_unknown(
        storage.as_ref(),
        MarkToolOutcomeUnknownRequest {
            scope: scope(),
            invocation_id: record.execution.invocation_id,
            now: Utc::now(),
        },
    )
    .await
    .unwrap_err();
    assert!(error.to_string().contains("irreversible"));
}

#[tokio::test]
async fn explicit_unknown_outcome_is_recorded_idempotently() {
    let (_directory, storage) = storage("project-1").await;
    let batch = prepare_tool_batch(storage.as_ref(), prepare_request())
        .await
        .unwrap();
    let invocation_id = batch.calls[1].invocation_id.clone();
    approve(storage.as_ref(), invocation_id.clone()).await;
    begin_tool_execution(
        storage.as_ref(),
        BeginToolExecutionRequest {
            scope: scope(),
            invocation_id: invocation_id.clone(),
            now: Utc::now(),
        },
    )
    .await
    .unwrap();
    let request = MarkToolOutcomeUnknownRequest {
        scope: scope(),
        invocation_id,
        now: Utc::now(),
    };
    let first = mark_tool_outcome_unknown(storage.as_ref(), request.clone())
        .await
        .unwrap();
    let repeated = mark_tool_outcome_unknown(storage.as_ref(), request)
        .await
        .unwrap();
    assert_eq!(first, repeated);
    assert_eq!(first.execution.status, ToolExecutionStatus::OutcomeUnknown);
    let semantic = read_semantic_state(storage.as_ref()).await;
    assert_eq!(semantic.messages.len(), 1);
    assert_eq!(semantic.outbox_count, 1);
}

#[tokio::test]
async fn rejected_tool_is_terminal_and_cannot_be_approved_or_started() {
    let (_directory, storage) = storage("project-1").await;
    let batch = prepare_tool_batch(storage.as_ref(), prepare_request())
        .await
        .unwrap();
    let invocation_id = batch.calls[1].invocation_id.clone();
    let rejected = decide_tool_approval(
        storage.as_ref(),
        DecideToolApprovalRequest {
            scope: scope(),
            invocation_id: invocation_id.clone(),
            decision: ToolApprovalDecision::Reject,
            reason: Some("the requested write was not authorized".to_string()),
            now: Utc::now(),
        },
    )
    .await
    .unwrap();
    assert_eq!(rejected.execution.status, ToolExecutionStatus::Rejected);
    assert!(rejected.execution.started_at.is_none());
    assert!(rejected.execution.completed_at.is_some());
    assert!(matches!(
        begin_tool_execution(
            storage.as_ref(),
            BeginToolExecutionRequest {
                scope: scope(),
                invocation_id: invocation_id.clone(),
                now: Utc::now(),
            },
        )
        .await
        .unwrap(),
        BeginToolExecutionResult::AlreadyCompleted(_)
    ));
    let conflict = decide_tool_approval(
        storage.as_ref(),
        DecideToolApprovalRequest {
            scope: scope(),
            invocation_id,
            decision: ToolApprovalDecision::Approve,
            reason: Some("changed my mind".to_string()),
            now: Utc::now(),
        },
    )
    .await
    .unwrap_err();
    assert!(conflict.to_string().contains("conflicts"));
    let semantic = read_semantic_state(storage.as_ref()).await;
    assert_eq!(semantic.messages.len(), 1);
    assert_eq!(
        semantic.messages[0].structured_payload.as_ref().unwrap()["status"],
        "rejected"
    );
}

struct SeedConflictingToolMessage {
    record_id: String,
    now: chrono::DateTime<Utc>,
}

#[async_trait]
impl StorageTransaction for SeedConflictingToolMessage {
    async fn execute(
        &mut self,
        repositories: &mut dyn TransactionRepositories,
    ) -> StorageResult<()> {
        repositories
            .agent_messages()
            .put(PutRecord {
                record: AgentMessageStateRecord {
                    metadata: metadata(&self.record_id, self.now),
                    message: AgentMessage {
                        record_id: self.record_id.clone(),
                        run_id: "run-1".to_string(),
                        thread_id: "task-1".to_string(),
                        turn_id: "conflict".to_string(),
                        sequence: 1,
                        role: AgentMessageRole::User,
                        content: Some("conflicting message".to_string()),
                        reasoning: None,
                        structured_payload: None,
                        tool_call_id: None,
                        response_id: None,
                        message_mode: MessageMode::Semantic,
                        message_source: "test".to_string(),
                        memory_sync_status: MemorySyncStatus::Pending,
                        created_at: self.now,
                    },
                },
                expected_revision: None,
            })
            .await?;
        Ok(())
    }
}

#[tokio::test]
async fn message_conflict_rolls_back_tool_completion() {
    let (_directory, storage) = storage("project-1").await;
    let batch = prepare_tool_batch(storage.as_ref(), prepare_request())
        .await
        .unwrap();
    let invocation_id = batch.calls[0].invocation_id.clone();
    begin_tool_execution(
        storage.as_ref(),
        BeginToolExecutionRequest {
            scope: scope(),
            invocation_id: invocation_id.clone(),
            now: Utc::now(),
        },
    )
    .await
    .unwrap();
    storage
        .transaction(&mut SeedConflictingToolMessage {
            record_id: tool_result_message_id(&invocation_id),
            now: Utc::now(),
        })
        .await
        .unwrap();

    let error = complete_tool_execution(
        storage.as_ref(),
        CompleteToolExecutionRequest {
            scope: scope(),
            invocation_id,
            status: ToolExecutionStatus::Succeeded,
            bounded_result: json!({"content": "ok"}),
            now: Utc::now(),
        },
    )
    .await
    .unwrap_err();
    assert!(error.to_string().contains("conflict"));
    let state = inspect_tool_batch(storage.as_ref(), scope(), &batch)
        .await
        .unwrap();
    assert_eq!(
        state.records[0].execution.status,
        ToolExecutionStatus::Started
    );
    assert_eq!(read_semantic_state(storage.as_ref()).await.outbox_count, 0);
}

#[tokio::test]
async fn project_scope_mismatch_fails_before_any_execution_is_frozen() {
    let (_directory, storage) = storage("project-2").await;
    let error = prepare_tool_batch(storage.as_ref(), prepare_request())
        .await
        .unwrap_err();
    assert!(error.to_string().contains("project scope"));
}
