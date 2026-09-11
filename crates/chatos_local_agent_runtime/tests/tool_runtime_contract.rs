// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::Arc;

use async_trait::async_trait;
use chatos_client_storage::{
    AgentEventStateRecord, AgentRunStateRecord, ClientStorage, PutRecord, RecordMetadata,
    RecordScope, SecretReference, SqliteBootstrapProfile, SqliteClientStorage,
    StorageEncryptionKey, StorageResult, StorageTransaction, TransactionRepositories,
};
use chatos_local_agent_protocol::{
    ContextStrategy, LocalAgentEvent, LocalAgentEventStatus, LocalAgentEventType, LocalAgentRun,
    LocalAgentRunStatus, ModelProtocol, ModelRuntimeDescriptor, ToolExecutionStatus,
};
use chatos_local_agent_runtime::{
    begin_tool_execution, complete_tool_execution, inspect_tool_batch, prepare_tool_batch,
    BeginToolExecutionRequest, BeginToolExecutionResult, CompleteToolExecutionRequest,
    PrepareToolBatchRequest,
};
use chrono::{Duration, Utc};
use serde_json::json;

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
    assert_eq!(first.calls.len(), 2);
    assert_eq!(first.calls[0].tool_name, "read_file");
    assert!(first.calls[0].invocation_id.starts_with("tool:"));
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
}

#[tokio::test]
async fn irreversible_started_execution_is_never_replayed_after_reentry() {
    let (_directory, storage) = storage("project-1").await;
    let batch = prepare_tool_batch(storage.as_ref(), prepare_request())
        .await
        .unwrap();
    let invocation_id = batch.calls[1].invocation_id.clone();
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
}

#[tokio::test]
async fn project_scope_mismatch_fails_before_any_execution_is_frozen() {
    let (_directory, storage) = storage("project-2").await;
    let error = prepare_tool_batch(storage.as_ref(), prepare_request())
        .await
        .unwrap_err();
    assert!(error.to_string().contains("project scope"));
}
