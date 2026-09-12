// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use chatos_client_storage::{
    decode_storage_archive, AgentRunStateRecord, ClientStorage, ClipboardRecord, PutRecord,
    RecordMetadata, RecordScope, SecretReference, SqliteBootstrapProfile, SqliteClientStorage,
    StorageEncryptionKey, StorageResult, StorageTransaction, TransactionRepositories,
};
use chatos_local_agent_host::{
    LocalAgentIpcMutationExecutor, LocalAgentStorageIpcExecutor, LocalAgentStoragePlatform,
};
use chatos_local_agent_protocol::{
    ApplyStorageProfileCommand, ClientStorageBackendKind, ClientStorageHealth,
    ClientStorageProfileDescriptor, ClientStorageProfileSelection, ContextStrategy,
    ExportClientDataCommand, ImportClientDataCommand, LocalAgentCommand, LocalAgentIpcError,
    LocalAgentIpcResponse, LocalAgentRun, LocalAgentRunStatus, ModelProtocol,
    ModelRuntimeDescriptor, PostgresConnectionTestCommand, PostgresConnectionTestResult,
};
use chrono::Utc;
use sha2::{Digest, Sha256};

#[derive(Default)]
struct PlatformState {
    archive: Option<Vec<u8>>,
    staged_profiles: Vec<ClientStorageProfileSelection>,
}

struct Platform {
    state: Mutex<PlatformState>,
}

#[async_trait]
impl LocalAgentStoragePlatform for Platform {
    async fn current_profile(&self) -> Result<ClientStorageProfileDescriptor, String> {
        Ok(sqlite_descriptor(ClientStorageHealth::Active))
    }

    async fn test_postgres(
        &self,
        connection_secret_reference: &str,
    ) -> Result<PostgresConnectionTestResult, String> {
        assert_eq!(connection_secret_reference, "keychain:postgres-1");
        Ok(PostgresConnectionTestResult {
            server_version: "PostgreSQL-16.4".to_string(),
            tls_active: true,
            authentication_ok: true,
            transaction_ok: true,
            migration_permission_ok: true,
        })
    }

    async fn stage_profile(
        &self,
        profile: &ClientStorageProfileSelection,
    ) -> Result<ClientStorageProfileDescriptor, String> {
        self.state
            .lock()
            .unwrap()
            .staged_profiles
            .push(profile.clone());
        Ok(ClientStorageProfileDescriptor {
            backend: ClientStorageBackendKind::Postgres,
            health: ClientStorageHealth::RestartRequired,
            sqlite_database_reference: None,
            postgres_connection_secret_reference: Some("keychain:postgres-1".to_string()),
            schema_version: 4,
            last_error_code: None,
        })
    }

    async fn write_archive(
        &self,
        destination_reference: &str,
        archive: &[u8],
    ) -> Result<String, String> {
        assert_eq!(destination_reference, "save-panel:archive-1");
        self.state.lock().unwrap().archive = Some(archive.to_vec());
        Ok("archive-grant:archive-1".to_string())
    }

    async fn read_archive(&self, source_reference: &str) -> Result<Vec<u8>, String> {
        assert_eq!(source_reference, "open-panel:archive-1");
        self.state
            .lock()
            .unwrap()
            .archive
            .clone()
            .ok_or_else(|| "archive unavailable".to_string())
    }
}

struct RejectNext;

#[async_trait]
impl LocalAgentIpcMutationExecutor for RejectNext {
    async fn execute_mutation(
        &self,
        _request_id: &str,
        _command: LocalAgentCommand,
    ) -> Result<LocalAgentIpcResponse, LocalAgentIpcError> {
        Err(LocalAgentIpcError {
            code: "unsupported".to_string(),
            message: "unsupported".to_string(),
            retryable: false,
        })
    }
}

fn scope() -> RecordScope {
    RecordScope {
        owner_user_id: "user-1".to_string(),
    }
}

fn sqlite_descriptor(health: ClientStorageHealth) -> ClientStorageProfileDescriptor {
    ClientStorageProfileDescriptor {
        backend: ClientStorageBackendKind::Sqlite,
        health,
        sqlite_database_reference: Some("app-container:client.sqlite3".to_string()),
        postgres_connection_secret_reference: None,
        schema_version: 4,
        last_error_code: None,
    }
}

async fn storage(key: u8) -> Arc<dyn ClientStorage> {
    let directory = tempfile::tempdir().unwrap().keep();
    Arc::new(
        SqliteClientStorage::open(
            &SqliteBootstrapProfile {
                database_path: directory.join("client.sqlite3"),
                encryption_secret: SecretReference::new(format!("test:storage-ipc-{key}")).unwrap(),
            },
            &StorageEncryptionKey::new([key; 32]),
        )
        .await
        .unwrap(),
    )
}

fn executor(
    storage: Arc<dyn ClientStorage>,
    platform: Arc<Platform>,
) -> LocalAgentStorageIpcExecutor {
    LocalAgentStorageIpcExecutor::new(storage, scope(), platform, Arc::new(RejectNext))
}

#[tokio::test]
async fn profile_and_postgres_test_use_only_opaque_platform_references() {
    let platform = Arc::new(Platform {
        state: Mutex::new(PlatformState::default()),
    });
    let executor = executor(storage(81).await, platform);

    assert!(matches!(
        executor
            .execute_mutation("request-1", LocalAgentCommand::GetStorageProfile)
            .await
            .unwrap(),
        LocalAgentIpcResponse::StorageProfile(ClientStorageProfileDescriptor {
            backend: ClientStorageBackendKind::Sqlite,
            health: ClientStorageHealth::Active,
            ..
        })
    ));
    let tested = executor
        .execute_mutation(
            "request-2",
            LocalAgentCommand::TestPostgresConnection(PostgresConnectionTestCommand {
                connection_secret_reference: "keychain:postgres-1".to_string(),
            }),
        )
        .await
        .unwrap();
    assert!(matches!(
        tested,
        LocalAgentIpcResponse::PostgresConnectionTest(PostgresConnectionTestResult {
            tls_active: true,
            transaction_ok: true,
            migration_permission_ok: true,
            ..
        })
    ));
}

#[tokio::test]
async fn storage_switch_is_staged_only_when_every_run_is_terminal() {
    let platform = Arc::new(Platform {
        state: Mutex::new(PlatformState::default()),
    });
    let storage = storage(82).await;
    let executor = executor(storage.clone(), platform.clone());
    let command = || {
        LocalAgentCommand::ApplyStorageProfile(ApplyStorageProfileCommand {
            profile: ClientStorageProfileSelection::Postgres {
                connection_secret_reference: "keychain:postgres-1".to_string(),
            },
            confirm_no_active_runs: true,
        })
    };
    let response = executor
        .execute_mutation("request-1", command())
        .await
        .unwrap();
    assert!(matches!(
        response,
        LocalAgentIpcResponse::StorageProfile(ClientStorageProfileDescriptor {
            health: ClientStorageHealth::RestartRequired,
            ..
        })
    ));
    storage
        .transaction(&mut SeedRun {
            status: LocalAgentRunStatus::ModelReady,
        })
        .await
        .unwrap();
    let error = executor
        .execute_mutation("request-2", command())
        .await
        .unwrap_err();
    assert_eq!(error.code, "active_runs_present");
    assert_eq!(platform.state.lock().unwrap().staged_profiles.len(), 1);
}

#[tokio::test]
async fn archive_export_strips_local_payload_references_and_import_checks_digest() {
    let source = storage(83).await;
    source.transaction(&mut SeedClipboard).await.unwrap();
    let source_platform = Arc::new(Platform {
        state: Mutex::new(PlatformState::default()),
    });
    let response = executor(source, source_platform.clone())
        .execute_mutation(
            "request-export",
            LocalAgentCommand::ExportClientData(ExportClientDataCommand {
                destination_reference: "save-panel:archive-1".to_string(),
                include_large_payload_references: false,
            }),
        )
        .await
        .unwrap();
    let (digest, record_count) = match response {
        LocalAgentIpcResponse::DataTransfer(result) => (result.archive_digest, result.record_count),
        other => panic!("unexpected response: {other:?}"),
    };
    assert_eq!(record_count, 1);
    let bytes = source_platform
        .state
        .lock()
        .unwrap()
        .archive
        .clone()
        .unwrap();
    let decoded = decode_storage_archive(bytes.as_slice()).unwrap();
    assert_eq!(decoded.records.clipboard[0].payload_reference, None);

    let target_platform = Arc::new(Platform {
        state: Mutex::new(PlatformState {
            archive: Some(bytes),
            staged_profiles: Vec::new(),
        }),
    });
    let target = storage(84).await;
    let response = executor(target.clone(), target_platform)
        .execute_mutation(
            "request-import",
            LocalAgentCommand::ImportClientData(ImportClientDataCommand {
                source_reference: "open-panel:archive-1".to_string(),
                expected_archive_digest: digest,
                confirm_no_active_runs: true,
            }),
        )
        .await
        .unwrap();
    assert!(matches!(response, LocalAgentIpcResponse::DataTransfer(_)));
}

struct SeedClipboard;

#[async_trait]
impl StorageTransaction for SeedClipboard {
    async fn execute(
        &mut self,
        repositories: &mut dyn TransactionRepositories,
    ) -> StorageResult<()> {
        let now = Utc::now();
        repositories
            .clipboard()
            .put(PutRecord {
                record: ClipboardRecord {
                    metadata: metadata("clipboard-1", now),
                    mime_type: "image/png".to_string(),
                    content_hash: format!("sha256:{:x}", Sha256::digest(b"image")),
                    payload_reference: Some("clipboard-grant:image-1".to_string()),
                    byte_size: 5,
                    state: serde_json::json!({}),
                },
                expected_revision: None,
            })
            .await?;
        Ok(())
    }
}

struct SeedRun {
    status: LocalAgentRunStatus,
}

#[async_trait]
impl StorageTransaction for SeedRun {
    async fn execute(
        &mut self,
        repositories: &mut dyn TransactionRepositories,
    ) -> StorageResult<()> {
        let now = Utc::now();
        let descriptor = ModelRuntimeDescriptor {
            model_config_id: "model-1".to_string(),
            revision: 1,
            provider: "openai".to_string(),
            model: "gpt-test".to_string(),
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
                    metadata: metadata("run-1", now),
                    run: LocalAgentRun {
                        run_id: "run-1".to_string(),
                        profile_key: "main_chat".to_string(),
                        owner_user_id: "user-1".to_string(),
                        owner_entity_type: "thread".to_string(),
                        owner_entity_id: "thread-1".to_string(),
                        project_id: None,
                        status: self.status,
                        version: 1,
                        step_seq: 0,
                        iteration: 0,
                        retry_count: 0,
                        model_config_id: descriptor.model_config_id.clone(),
                        model_config_revision: descriptor.revision,
                        model_runtime_snapshot: descriptor,
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
                expected_revision: None,
            })
            .await?;
        Ok(())
    }
}

fn metadata(id: &str, now: chrono::DateTime<Utc>) -> RecordMetadata {
    RecordMetadata {
        id: id.to_string(),
        scope: scope(),
        origin_device_id: "device-1".to_string(),
        revision: 1,
        created_at: now,
        updated_at: now,
    }
}
