// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

#![cfg(unix)]

use std::io::Cursor;
use std::sync::Arc;

use async_trait::async_trait;
use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use chatos_local_agent_host::{
    run_local_agent_host_process, LocalAgentHostAssemblyDependencies, LocalAgentHostReady,
    LocalAgentIpcMutationExecutor, LocalAgentStoragePlatform, RegisteredLocalCapabilityRuntime,
    LOCAL_AGENT_HOST_LAUNCH_PROTOCOL_VERSION,
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

#[tokio::test]
async fn announces_ready_only_after_assembly_and_stops_cleanly() {
    let directory = tempfile::tempdir().unwrap();
    let socket = directory.path().join("agent.sock");
    let database = directory.path().join("client.sqlite");
    let grants = directory.path().join("attachment-grants");
    std::fs::create_dir(&grants).unwrap();
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&grants, std::fs::Permissions::from_mode(0o700)).unwrap();
    }
    let launch = json!({
        "protocol_version": LOCAL_AGENT_HOST_LAUNCH_PROTOCOL_VERSION,
        "launch_id": "launch-process-1",
        "owner_user_id": "user-1",
        "device_id": "device-1",
        "worker_id": "worker-1",
        "ipc_endpoint": { "transport": "unix_socket", "path": socket },
        "attachment_grant_directory": grants,
        "model_gateway_base_url": "https://api.example.com",
        "memory_engine_base_url": "https://memory.example.com",
        "memory_source_id": "local-agent",
        "storage_profile": {
            "backend": "sqlite",
            "database_path": database,
            "encryption_secret": "sqlite-secret-1",
        },
        "credentials": {
            "model_access_token": "private-model-token",
            "provider_context_key_base64": STANDARD.encode([3_u8; 32]),
            "storage": {
                "backend": "sqlite",
                "encryption_secret_reference": "sqlite-secret-1",
                "encryption_key_base64": STANDARD.encode([7_u8; 32]),
            },
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
                storage_platform: Arc::new(Platform),
                terminal_mutation_executor: Arc::new(Terminal),
                capability_runtime: Arc::new(RegisteredLocalCapabilityRuntime::new()),
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
