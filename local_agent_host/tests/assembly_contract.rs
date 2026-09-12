// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

#![cfg(unix)]

use std::io::Cursor;
use std::sync::Arc;

use async_trait::async_trait;
use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use chatos_local_agent_host::{
    assemble_local_agent_host, read_local_agent_host_launch_request,
    LocalAgentHostAssemblyDependencies, LocalAgentIpcMutationExecutor, LocalAgentStoragePlatform,
    RegisteredLocalCapabilityRuntime, LOCAL_AGENT_HOST_LAUNCH_PROTOCOL_VERSION,
};
use chatos_local_agent_protocol::{
    ClientStorageProfileDescriptor, ClientStorageProfileSelection, LocalAgentCommand,
    LocalAgentIpcError, LocalAgentIpcRequest, LocalAgentIpcResponse, PostgresConnectionTestResult,
    LOCAL_AGENT_PROTOCOL_VERSION,
};
use serde_json::json;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixStream;
use tokio_util::sync::CancellationToken;

struct Platform;

#[async_trait]
impl LocalAgentStoragePlatform for Platform {
    async fn current_profile(&self) -> Result<ClientStorageProfileDescriptor, String> {
        Err("not invoked by assembly test".to_string())
    }

    async fn test_postgres(
        &self,
        _connection_secret_reference: &str,
    ) -> Result<PostgresConnectionTestResult, String> {
        Err("not invoked by assembly test".to_string())
    }

    async fn stage_profile(
        &self,
        _profile: &ClientStorageProfileSelection,
    ) -> Result<ClientStorageProfileDescriptor, String> {
        Err("not invoked by assembly test".to_string())
    }

    async fn write_archive(
        &self,
        _destination_reference: &str,
        _archive: &[u8],
    ) -> Result<String, String> {
        Err("not invoked by assembly test".to_string())
    }

    async fn read_archive(&self, _source_reference: &str) -> Result<Vec<u8>, String> {
        Err("not invoked by assembly test".to_string())
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
            message: "not invoked by assembly test".to_string(),
            retryable: false,
        })
    }
}

#[tokio::test]
async fn assembles_one_storage_runtime_worker_and_protected_ipc_listener() {
    let directory = tempfile::tempdir().unwrap();
    let socket_path = directory.path().join("local-agent.sock");
    let database_path = directory.path().join("client.sqlite");
    let grant_directory = directory.path().join("attachment-grants");
    let platform_state_directory = directory.path().join("platform-state");
    std::fs::create_dir(&grant_directory).unwrap();
    std::fs::create_dir(&platform_state_directory).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&grant_directory, std::fs::Permissions::from_mode(0o700)).unwrap();
        std::fs::set_permissions(
            &platform_state_directory,
            std::fs::Permissions::from_mode(0o700),
        )
        .unwrap();
    }
    let launch = json!({
        "protocol_version": LOCAL_AGENT_HOST_LAUNCH_PROTOCOL_VERSION,
        "launch_id": "launch-assembly-1",
        "owner_user_id": "user-1",
        "device_id": "device-1",
        "worker_id": "worker-1",
        "ipc_endpoint": {
            "transport": "unix_socket",
            "path": socket_path,
        },
        "attachment_grant_directory": grant_directory,
        "platform_state_directory": platform_state_directory,
        "model_gateway_base_url": "https://api.example.com",
        "memory_engine_base_url": "https://memory.example.com",
        "memory_source_id": "local-agent",
        "storage_profile": {
            "backend": "sqlite",
            "database_path": database_path,
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
    let request = read_local_agent_host_launch_request(&mut Cursor::new(frame))
        .await
        .unwrap();
    let capability_runtime = Arc::new(RegisteredLocalCapabilityRuntime::new());

    let assembly = assemble_local_agent_host(
        &request,
        LocalAgentHostAssemblyDependencies {
            storage_platform: Arc::new(Platform),
            terminal_mutation_executor: Arc::new(Terminal),
            capability_runtime: capability_runtime.clone(),
        },
    )
    .await
    .unwrap();

    assert_eq!(assembly.startup_report.active_run_count, 0);
    assert_eq!(assembly.startup_report.ready_event_count, 0);
    assert!(Arc::ptr_eq(
        &assembly.capability_runtime,
        &capability_runtime
    ));
    assert_eq!(assembly.client_endpoint, socket_path.to_str().unwrap());
    assert!(socket_path.exists());

    let shutdown = CancellationToken::new();
    let service_shutdown = shutdown.clone();
    let service = tokio::spawn(async move { assembly.service.run(service_shutdown).await });
    let mut client = UnixStream::connect(&socket_path).await.unwrap();
    let request = LocalAgentIpcRequest {
        protocol_version: LOCAL_AGENT_PROTOCOL_VERSION,
        request_id: "request-1".to_string(),
        owner_user_id: "user-1".to_string(),
        command: LocalAgentCommand::GetRun {
            run_id: "missing-run".to_string(),
        },
    };
    let body = serde_json::to_vec(&request).unwrap();
    client.write_u32(body.len() as u32).await.unwrap();
    client.write_all(body.as_slice()).await.unwrap();
    let length = client.read_u32().await.unwrap();
    let mut reply = vec![0_u8; length as usize];
    client.read_exact(reply.as_mut_slice()).await.unwrap();
    let reply: chatos_local_agent_protocol::LocalAgentIpcReply =
        serde_json::from_slice(reply.as_slice()).unwrap();
    assert_eq!(reply.request_id, "request-1");
    assert!(matches!(reply.response, LocalAgentIpcResponse::Error(_)));

    shutdown.cancel();
    service.await.unwrap().unwrap();
}
