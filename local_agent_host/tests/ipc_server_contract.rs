// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use chatos_client_storage::{
    AgentRunStateRecord, AppendAgentUiEvent, ClientStorage, PutRecord, RecordMetadata, RecordScope,
    SecretReference, SqliteBootstrapProfile, SqliteClientStorage, StorageEncryptionKey,
    StorageResult, StorageTransaction, TransactionRepositories,
};
use chatos_local_agent_host::{
    LocalAgentIpcMutationExecutor, LocalAgentIpcServer, LocalAgentIpcServerError,
};
use chatos_local_agent_protocol::{
    ContextStrategy, LocalAgentCommand, LocalAgentHostState, LocalAgentHostUiStatus,
    LocalAgentIpcError, LocalAgentIpcReply, LocalAgentIpcRequest, LocalAgentIpcResponse,
    LocalAgentRun, LocalAgentRunStatus, LocalAgentUiEventPayload, ModelProtocol,
    ModelRuntimeDescriptor, LOCAL_AGENT_PROTOCOL_VERSION,
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
        let run = run();
        repositories
            .agent_runs()
            .put(PutRecord {
                record: AgentRunStateRecord {
                    metadata: RecordMetadata {
                        id: run.run_id.clone(),
                        scope: scope(),
                        origin_device_id: "device-1".to_string(),
                        revision: 0,
                        created_at: run.created_at,
                        updated_at: run.updated_at,
                    },
                    run,
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
