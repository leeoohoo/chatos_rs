// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

#![cfg(unix)]

use std::io::Cursor;
use std::sync::Arc;

use async_trait::async_trait;
use chatos_client_storage::{
    PostgresConnectionSettings, SecretReference, StorageEncryptionKey, StorageError, StorageResult,
    StorageSecretResolver,
};
use chatos_local_agent_host::{
    native_local_agent_host_exit_code, run_local_agent_host_process,
    LocalAgentHostAssemblyDependencies, LocalAgentHostError, LocalAgentHostProcessError,
    LocalAgentHostReady, LocalAgentHostResolvedCredentials, LocalAgentHostServiceError,
    LocalAgentIpcMutationExecutor, LocalAgentMemorySyncWorkerError, LocalAgentStoragePlatform,
    LocalCapabilityPlatform, NativeLocalAgentHostProcessError,
    LOCAL_AGENT_HOST_LAUNCH_PROTOCOL_VERSION, NATIVE_LOCAL_AGENT_HOST_GENERAL_FAILURE_EXIT_CODE,
    NATIVE_LOCAL_AGENT_HOST_STORAGE_UNAVAILABLE_EXIT_CODE,
};
use chatos_local_agent_protocol::{
    ClientStorageProfileDescriptor, ClientStorageProfileSelection, LocalAgentCommand,
    LocalAgentIpcError, LocalAgentIpcResponse, PostgresConnectionTestResult,
};
use serde_json::json;
use tokio::io::{AsyncReadExt, DuplexStream};
use tokio_util::sync::CancellationToken;

struct Platform;

#[async_trait]
impl LocalAgentStoragePlatform for Platform {
    async fn current_profile(&self) -> Result<ClientStorageProfileDescriptor, String> {
        Err("not invoked by process test".to_string())
    }

    async fn test_postgres(
        &self,
        _connection_secret_reference: &str,
    ) -> Result<PostgresConnectionTestResult, String> {
        Err("not invoked by process test".to_string())
    }

    async fn stage_profile(
        &self,
        _profile: &ClientStorageProfileSelection,
    ) -> Result<ClientStorageProfileDescriptor, String> {
        Err("not invoked by process test".to_string())
    }

    async fn write_archive(
        &self,
        _destination_reference: &str,
        _archive: &[u8],
    ) -> Result<String, String> {
        Err("not invoked by process test".to_string())
    }

    async fn read_archive(&self, _source_reference: &str) -> Result<Vec<u8>, String> {
        Err("not invoked by process test".to_string())
    }
}

#[async_trait]
impl LocalCapabilityPlatform for Platform {
    async fn resolve_plugin_executable(
        &self,
        _reference: &str,
        _expected_sha256: &str,
    ) -> Result<std::path::PathBuf, String> {
        Err("not invoked by process test".to_string())
    }

    async fn resolve_plugin_environment_secret(&self, _reference: &str) -> Result<String, String> {
        Err("not invoked by process test".to_string())
    }
}

struct Terminal;

#[async_trait]
impl LocalAgentIpcMutationExecutor for Terminal {
    async fn execute_mutation(
        &self,
        _request_id: &str,
        _command: LocalAgentCommand,
    ) -> Result<LocalAgentIpcResponse, LocalAgentIpcError> {
        Err(LocalAgentIpcError {
            code: "unsupported_test_command".to_string(),
            message: "not invoked by process test".to_string(),
            retryable: false,
        })
    }
}

struct Secrets;

#[test]
fn exposes_storage_unavailability_as_a_stable_native_exit_code() {
    let worker = NativeLocalAgentHostProcessError::Process(LocalAgentHostProcessError::Service(
        LocalAgentHostServiceError::Worker(LocalAgentHostError::Storage(
            StorageError::Unavailable {
                reason: "database offline".to_string(),
            },
        )),
    ));
    let memory_sync = NativeLocalAgentHostProcessError::Process(
        LocalAgentHostProcessError::Service(LocalAgentHostServiceError::MemorySync(
            LocalAgentMemorySyncWorkerError::Storage(StorageError::Unavailable {
                reason: "database offline".to_string(),
            }),
        )),
    );
    let general = NativeLocalAgentHostProcessError::Process(LocalAgentHostProcessError::Service(
        LocalAgentHostServiceError::UnexpectedWorkerExit,
    ));

    assert_eq!(
        native_local_agent_host_exit_code(&worker),
        NATIVE_LOCAL_AGENT_HOST_STORAGE_UNAVAILABLE_EXIT_CODE
    );
    assert_eq!(
        native_local_agent_host_exit_code(&memory_sync),
        NATIVE_LOCAL_AGENT_HOST_STORAGE_UNAVAILABLE_EXIT_CODE
    );
    assert_eq!(
        native_local_agent_host_exit_code(&general),
        NATIVE_LOCAL_AGENT_HOST_GENERAL_FAILURE_EXIT_CODE
    );
}

#[async_trait]
impl StorageSecretResolver for Secrets {
    async fn resolve_sqlite_encryption_key(
        &self,
        _reference: &SecretReference,
    ) -> StorageResult<StorageEncryptionKey> {
        Ok(StorageEncryptionKey::new([7_u8; 32]))
    }

    async fn resolve_postgres(
        &self,
        _reference: &SecretReference,
    ) -> StorageResult<PostgresConnectionSettings> {
        Err(StorageError::Unavailable {
            reason: "PostgreSQL is not used by this test".to_string(),
        })
    }
}

#[tokio::test]
async fn announces_ready_only_after_assembly_and_stops_cleanly() {
    let directory = tempfile::tempdir().unwrap();
    let socket = directory.path().join("agent.sock");
    let database = directory.path().join("client.sqlite");
    let grants = directory.path().join("attachment-grants");
    let platform_state = directory.path().join("platform-state");
    std::fs::create_dir(&grants).unwrap();
    std::fs::create_dir(&platform_state).unwrap();
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&grants, std::fs::Permissions::from_mode(0o700)).unwrap();
        std::fs::set_permissions(&platform_state, std::fs::Permissions::from_mode(0o700)).unwrap();
    }
    let launch = json!({
        "protocol_version": LOCAL_AGENT_HOST_LAUNCH_PROTOCOL_VERSION,
        "launch_id": "launch-process-1",
        "owner_user_id": "user-1",
        "device_id": "device-1",
        "worker_id": "worker-1",
        "ipc_endpoint": { "transport": "unix_socket", "path": socket },
        "attachment_grant_directory": grants,
        "platform_state_directory": platform_state,
        "model_gateway_base_url": "https://api.example.com",
        "memory_engine_base_url": "https://memory.example.com",
        "memory_source_id": "local-agent",
        "storage_profile": {
            "backend": "sqlite",
            "database_path": database,
            "encryption_secret": "sqlite-secret-1",
        },
        "credential_references": {
            "model_access_token_reference": "model-access-token",
            "provider_context_key_reference": "provider-context-key",
        },
    });
    let body = serde_json::to_vec(&launch).unwrap();
    let mut frame = Vec::with_capacity(body.len() + 4);
    frame.extend_from_slice(&(body.len() as u32).to_be_bytes());
    frame.extend_from_slice(body.as_slice());
    let mut input = Cursor::new(frame);
    let (mut ready_reader, mut ready_writer): (DuplexStream, DuplexStream) =
        tokio::io::duplex(4096);
    let shutdown = CancellationToken::new();
    let process_shutdown = shutdown.clone();
    let process = tokio::spawn(async move {
        run_local_agent_host_process(
            &mut input,
            &mut ready_writer,
            LocalAgentHostAssemblyDependencies {
                credentials: LocalAgentHostResolvedCredentials::new(
                    "private-model-token",
                    [3_u8; 32],
                    Arc::new(Secrets),
                )
                .unwrap(),
                storage_platform: Arc::new(Platform),
                capability_platform: Arc::new(Platform),
                terminal_mutation_executor: Arc::new(Terminal),
            },
            process_shutdown,
        )
        .await
    });

    let length = ready_reader.read_u32().await.unwrap();
    let mut ready_body = vec![0_u8; length as usize];
    ready_reader
        .read_exact(ready_body.as_mut_slice())
        .await
        .unwrap();
    let ready: LocalAgentHostReady = serde_json::from_slice(&ready_body).unwrap();
    assert_eq!(
        ready.protocol_version,
        LOCAL_AGENT_HOST_LAUNCH_PROTOCOL_VERSION
    );
    assert_eq!(ready.launch_id, "launch-process-1");
    assert_eq!(ready.process_id, std::process::id());
    assert_eq!(ready.client_endpoint, socket.to_str().unwrap());
    assert!(socket.exists());

    shutdown.cancel();
    let exit = process.await.unwrap().unwrap();
    assert_eq!(exit.worker.processed_event_count, 0);
    assert!(!socket.exists());
}
