// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use chatos_client_storage::{
    AgentMessageStateRecord, AgentRunStateRecord, AppendAgentUiEvent, ClientStorage, PutRecord,
    RecordMetadata, RecordScope, SecretReference, SqliteBootstrapProfile, SqliteClientStorage,
    StorageEncryptionKey, StorageResult, StorageTransaction, TaskRecord, TransactionRepositories,
};
use chatos_local_agent_host::{
    LocalAgentIpcMutationExecutor, LocalAgentIpcServer, LocalAgentIpcServerError,
};
use chatos_local_agent_protocol::{
    AgentMessage, AgentMessageRole, ContextStrategy, LocalAgentCommand, LocalAgentHostState,
    LocalAgentHostUiStatus, LocalAgentIpcError, LocalAgentIpcReply, LocalAgentIpcRequest,
    LocalAgentIpcResponse, LocalAgentRun, LocalAgentRunStatus, LocalAgentUiEventPayload,
    MemorySyncStatus, MessageMode, ModelProtocol, ModelRuntimeDescriptor,
    LOCAL_AGENT_PROTOCOL_VERSION,
};
use chrono::Utc;

fn scope() -> RecordScope {
    RecordScope {
        owner_user_id: "user-1".to_string(),
    }
}

fn run() -> LocalAgentRun {
    let now = Utc::now();
    LocalAgentRun {
        run_id: "run-1".to_string(),
        profile_key: "main_chat".to_string(),
        owner_user_id: "user-1".to_string(),
        owner_entity_type: "conversation".to_string(),
        owner_entity_id: "thread-1".to_string(),
        project_id: None,
        status: LocalAgentRunStatus::Queued,
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
        pending_interaction: None,
        terminal_outcome: None,
        deadline_at: None,
        created_at: now,
        updated_at: now,
    }
}

struct Seed;

#[async_trait]
impl StorageTransaction for Seed {
    async fn execute(
        &mut self,
        repositories: &mut dyn TransactionRepositories,
    ) -> StorageResult<()> {
        let main_run = run();
        repositories
            .agent_runs()
            .put(PutRecord {
                record: AgentRunStateRecord {
                    metadata: RecordMetadata {
                        id: main_run.run_id.clone(),
                        scope: scope(),
                        origin_device_id: "device-1".to_string(),
                        revision: 0,
                        created_at: main_run.created_at,
                        updated_at: main_run.updated_at,
                    },
                    run: main_run,
                },
                expected_revision: None,
            })
            .await?;
        let mut task_run = run();
        task_run.run_id = "task-run-1".to_string();
        task_run.profile_key = "task_runner".to_string();
        task_run.owner_entity_type = "task".to_string();
        task_run.owner_entity_id = "task-1".to_string();
        task_run.project_id = Some("project-1".to_string());
        task_run.model_config_id = "task-model-1".to_string();
        task_run.model_runtime_snapshot.model_config_id = "task-model-1".to_string();
        repositories
            .agent_runs()
            .put(PutRecord {
                record: AgentRunStateRecord {
                    metadata: RecordMetadata {
                        id: task_run.run_id.clone(),
                        scope: scope(),
                        origin_device_id: "device-1".to_string(),
                        revision: 0,
                        created_at: task_run.created_at,
                        updated_at: task_run.updated_at,
                    },
                    run: task_run.clone(),
                },
                expected_revision: None,
            })
            .await?;
        repositories
            .tasks()
            .put(PutRecord {
                record: TaskRecord {
                    metadata: RecordMetadata {
                        id: "task-1".to_string(),
                        scope: scope(),
                        origin_device_id: "device-1".to_string(),
                        revision: 3,
                        created_at: task_run.created_at,
                        updated_at: task_run.updated_at,
                    },
                    conversation_id: Some("thread-1".to_string()),
                    status: "queued".to_string(),
                    state: serde_json::json!({
                        "source_thread_id": "thread-1",
                        "source_turn_id": "turn-1",
                        "project_id": "project-1",
                        "run_id": "task-run-1",
                        "objective": "Implement the approved visual design",
                        "acceptance_criteria": [
                            "The rendered UI matches the approved reference",
                            "The visual verification succeeds"
                        ],
                        "model_config_id": "task-model-1",
                        "model_config_revision": 1
                    }),
                },
                expected_revision: None,
            })
            .await?;
        repositories
            .agent_messages()
            .put(PutRecord {
                record: AgentMessageStateRecord {
                    metadata: RecordMetadata {
                        id: "message-1".to_string(),
                        scope: scope(),
                        origin_device_id: "device-1".to_string(),
                        revision: 0,
                        created_at: Utc::now(),
                        updated_at: Utc::now(),
                    },
                    message: AgentMessage {
                        record_id: "message-1".to_string(),
                        run_id: "run-1".to_string(),
                        thread_id: "thread-1".to_string(),
                        turn_id: "turn-1".to_string(),
                        sequence: 1,
                        role: AgentMessageRole::User,
                        content: Some("Design it".to_string()),
                        reasoning: None,
                        structured_payload: None,
                        tool_call_id: None,
                        response_id: None,
                        message_mode: MessageMode::Semantic,
                        message_source: "main_chat".to_string(),
                        memory_sync_status: MemorySyncStatus::Pending,
                        created_at: Utc::now(),
                    },
                },
                expected_revision: None,
            })
            .await?;
        repositories
            .agent_ui_events()
            .append(AppendAgentUiEvent {
                scope: scope(),
                origin_device_id: "device-1".to_string(),
                payload: LocalAgentUiEventPayload::HostStatus(LocalAgentHostUiStatus {
                    state: LocalAgentHostState::Ready,
                    active_run_count: 1,
                    error_code: None,
                }),
            })
            .await?;
        Ok(())
    }
}

struct RecordingMutationExecutor {
    calls: AtomicUsize,
}

#[async_trait]
impl LocalAgentIpcMutationExecutor for RecordingMutationExecutor {
    async fn execute_mutation(
        &self,
        _request_id: &str,
        _command: LocalAgentCommand,
    ) -> Result<LocalAgentIpcResponse, LocalAgentIpcError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(LocalAgentIpcResponse::Success)
    }
}

async fn server() -> (
    tempfile::TempDir,
    Arc<SqliteClientStorage>,
    Arc<RecordingMutationExecutor>,
    LocalAgentIpcServer,
) {
    let directory = tempfile::tempdir().unwrap();
    let storage = Arc::new(
        SqliteClientStorage::open(
            &SqliteBootstrapProfile {
                database_path: directory.path().join("client.sqlite3"),
                encryption_secret: SecretReference::new("test:ipc-key").unwrap(),
            },
            &StorageEncryptionKey::new([42; 32]),
        )
        .await
        .unwrap(),
    );
    storage.transaction(&mut Seed).await.unwrap();
    let executor = Arc::new(RecordingMutationExecutor {
        calls: AtomicUsize::new(0),
    });
    let server = LocalAgentIpcServer::new(storage.clone(), scope(), executor.clone()).unwrap();
    (directory, storage, executor, server)
}

fn request(request_id: &str, command: LocalAgentCommand) -> LocalAgentIpcRequest {
    LocalAgentIpcRequest {
        protocol_version: LOCAL_AGENT_PROTOCOL_VERSION,
        request_id: request_id.to_string(),
        owner_user_id: "user-1".to_string(),
        command,
    }
}

#[tokio::test]
async fn query_commands_are_owner_scoped_and_keep_request_correlation() {
    let (_directory, _storage, executor, server) = server().await;
    let reply = server
        .handle_request(request(
            "request-get",
            LocalAgentCommand::GetRun {
                run_id: "run-1".to_string(),
            },
        ))
        .await;
    assert_eq!(reply.request_id, "request-get");
    let LocalAgentIpcResponse::Run(run) = reply.response else {
        panic!("GetRun must return the owner-scoped Run");
    };
    assert_eq!(run.run_id, "run-1");

    let reply = server
        .handle_request(request(
            "request-events",
            LocalAgentCommand::SubscribeRunEvents {
                after_seq: 0,
                limit: 10,
            },
        ))
        .await;
    let LocalAgentIpcResponse::Events {
        events, next_seq, ..
    } = reply.response
    else {
        panic!("SubscribeRunEvents must return durable UI events");
    };
    assert_eq!(events.len(), 1);
    assert_eq!(next_seq, 1);
    assert_eq!(executor.calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn main_chat_binding_and_ui_cursor_are_storage_backed() {
    let (_directory, _storage, executor, server) = server().await;
    let reply = server
        .handle_request(request(
            "request-binding",
            LocalAgentCommand::GetMainChatRunBinding {
                run_id: "run-1".to_string(),
            },
        ))
        .await;
    let LocalAgentIpcResponse::MainChatRunBinding(binding) = reply.response else {
        panic!("Main Chat binding must come from the durable initial message");
    };
    assert_eq!(binding.thread_id, "thread-1");
    assert_eq!(binding.turn_id, "turn-1");
    assert_eq!(binding.message_id, "message-1");
    assert_eq!(binding.user_message.content.as_deref(), Some("Design it"));
    assert_eq!(binding.user_message.sequence, 1);

    let initial = server
        .handle_request(request(
            "request-cursor-initial",
            LocalAgentCommand::GetUiEventCursor,
        ))
        .await;
    assert!(matches!(
        initial.response,
        LocalAgentIpcResponse::UiEventCursor { event_seq: 0 }
    ));
    let acknowledged = server
        .handle_request(request(
            "request-cursor-ack",
            LocalAgentCommand::AcknowledgeUiEvents { through_seq: 1 },
        ))
        .await;
    assert!(matches!(
        acknowledged.response,
        LocalAgentIpcResponse::UiEventCursor { event_seq: 1 }
    ));
    let restored = server
        .handle_request(request(
            "request-cursor-restored",
            LocalAgentCommand::GetUiEventCursor,
        ))
        .await;
    assert!(matches!(
        restored.response,
        LocalAgentIpcResponse::UiEventCursor { event_seq: 1 }
    ));
    let impossible = server
        .handle_request(request(
            "request-cursor-impossible",
            LocalAgentCommand::AcknowledgeUiEvents { through_seq: 2 },
        ))
        .await;
    assert!(matches!(
        impossible.response,
        LocalAgentIpcResponse::Error(_)
    ));
    assert_eq!(executor.calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn task_queries_restore_the_frozen_owner_scoped_task_identity() {
    let (_directory, _storage, executor, server) = server().await;
    let reply = server
        .handle_request(request(
            "request-task",
            LocalAgentCommand::GetTask {
                task_id: "task-1".to_string(),
            },
        ))
        .await;
    let LocalAgentIpcResponse::Task(task) = reply.response else {
        panic!("GetTask must return the owner-scoped Task");
    };
    assert_eq!(task.task_id, "task-1");
    assert_eq!(task.revision, 1);
    assert_eq!(task.run_id, "task-run-1");
    assert_eq!(task.source_thread_id, "thread-1");
    assert_eq!(task.source_turn_id, "turn-1");
    assert_eq!(task.project_id, "project-1");
    assert_eq!(task.acceptance_criteria.len(), 2);

    let reply = server
        .handle_request(request(
            "request-tasks",
            LocalAgentCommand::ListTasks {
                cursor: None,
                limit: 10,
            },
        ))
        .await;
    let LocalAgentIpcResponse::Tasks { tasks, next_cursor } = reply.response else {
        panic!("ListTasks must return durable Task snapshots");
    };
    assert_eq!(tasks.as_slice(), [task.as_ref().clone()]);
    assert!(next_cursor.is_none());
    assert_eq!(executor.calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn owner_mismatch_fails_closed_before_queries_or_mutations() {
    let (_directory, _storage, executor, server) = server().await;
    let mut foreign = request("request-foreign", LocalAgentCommand::GetStorageProfile);
    foreign.owner_user_id = "user-2".to_string();
    let reply = server.handle_request(foreign).await;
    let LocalAgentIpcResponse::Error(error) = reply.response else {
        panic!("foreign owner must be rejected");
    };
    assert_eq!(error.code, "owner_scope_mismatch");
    assert_eq!(executor.calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn invalid_request_still_returns_a_valid_typed_reply() {
    let (_directory, _storage, executor, server) = server().await;
    let mut invalid = request("placeholder", LocalAgentCommand::GetStorageProfile);
    invalid.request_id.clear();
    let reply = server.handle_request(invalid).await;
    assert_eq!(reply.request_id, "invalid-request");
    assert!(matches!(reply.response, LocalAgentIpcResponse::Error(_)));
    reply.validate().unwrap();
    assert_eq!(executor.calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn mutation_commands_use_the_typed_executor() {
    let (_directory, _storage, executor, server) = server().await;
    let reply = server
        .handle_request(request(
            "request-mutation",
            LocalAgentCommand::GetStorageProfile,
        ))
        .await;
    assert!(matches!(reply.response, LocalAgentIpcResponse::Success));
    assert_eq!(executor.calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn frame_boundary_rejects_invalid_and_oversized_json() {
    let (_directory, storage, executor, _server) = server().await;
    let server =
        LocalAgentIpcServer::with_maximum_frame_bytes(storage, scope(), executor, 256).unwrap();
    assert!(matches!(
        server.handle_frame(b"not-json").await,
        Err(LocalAgentIpcServerError::InvalidJson(_))
    ));
    assert!(matches!(
        server.handle_frame(&vec![b'x'; 257]).await,
        Err(LocalAgentIpcServerError::RequestFrameTooLarge { .. })
    ));
}

#[tokio::test]
async fn oversized_reply_becomes_a_correlated_typed_error() {
    let (_directory, storage, executor, _server) = server().await;
    let server =
        LocalAgentIpcServer::with_maximum_frame_bytes(storage, scope(), executor, 512).unwrap();
    let frame = serde_json::to_vec(&request(
        "request-large-reply",
        LocalAgentCommand::GetRun {
            run_id: "run-1".to_string(),
        },
    ))
    .unwrap();
    let encoded = server.handle_frame(&frame).await.unwrap();
    let reply: LocalAgentIpcReply = serde_json::from_slice(&encoded).unwrap();
    assert_eq!(reply.request_id, "request-large-reply");
    let LocalAgentIpcResponse::Error(error) = reply.response else {
        panic!("large response must become a typed error");
    };
    assert_eq!(error.code, "ipc_response_too_large");
}
