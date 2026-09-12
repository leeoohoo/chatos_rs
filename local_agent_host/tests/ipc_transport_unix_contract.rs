// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

#![cfg(unix)]

use std::os::unix::fs::{FileTypeExt, PermissionsExt};
use std::sync::Arc;

use async_trait::async_trait;
use chatos_client_storage::{
    ClientStorage, RecordScope, SecretReference, SqliteBootstrapProfile, SqliteClientStorage,
    StorageEncryptionKey,
};
use chatos_local_agent_host::{
    LocalAgentIpcMutationExecutor, LocalAgentIpcServer, UnixLocalAgentIpcError,
    UnixLocalAgentIpcTransport,
};
use chatos_local_agent_protocol::{
    LocalAgentCommand, LocalAgentIpcError, LocalAgentIpcReply, LocalAgentIpcRequest,
    LocalAgentIpcResponse, LOCAL_AGENT_PROTOCOL_VERSION,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixStream;
use tokio_util::sync::CancellationToken;

struct RejectMutations;

#[async_trait]
impl LocalAgentIpcMutationExecutor for RejectMutations {
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

async fn server() -> Arc<LocalAgentIpcServer> {
    let directory = tempfile::tempdir().unwrap().keep();
    let storage: Arc<dyn ClientStorage> = Arc::new(
        SqliteClientStorage::open(
            &SqliteBootstrapProfile {
                database_path: directory.join("client.sqlite3"),
                encryption_secret: SecretReference::new("test:unix-ipc-key").unwrap(),
            },
            &StorageEncryptionKey::new([91; 32]),
        )
        .await
        .unwrap(),
    );
    Arc::new(LocalAgentIpcServer::new(storage, scope(), Arc::new(RejectMutations)).unwrap())
}

fn scope() -> RecordScope {
    RecordScope {
        owner_user_id: "user-1".to_string(),
    }
}

fn list_request() -> LocalAgentIpcRequest {
    LocalAgentIpcRequest {
        protocol_version: LOCAL_AGENT_PROTOCOL_VERSION,
        request_id: "request-1".to_string(),
        owner_user_id: "user-1".to_string(),
        command: LocalAgentCommand::ListRuns {
            cursor: None,
            limit: 20,
        },
    }
}

#[tokio::test]
async fn serves_one_length_delimited_request_over_a_private_same_user_socket() {
    let directory = tempfile::tempdir().unwrap();
    let socket_path = directory.path().join("local-agent.sock");
    let transport = UnixLocalAgentIpcTransport::bind(socket_path.clone(), server().await, unsafe {
        libc::geteuid()
    })
    .unwrap();
    let metadata = std::fs::symlink_metadata(&socket_path).unwrap();
    assert!(metadata.file_type().is_socket());
    assert_eq!(metadata.permissions().mode() & 0o077, 0);
    let cancellation = CancellationToken::new();
    let serving = tokio::spawn(transport.serve(cancellation.clone()));

    let mut stream = UnixStream::connect(&socket_path).await.unwrap();
    let request = serde_json::to_vec(&list_request()).unwrap();
    stream.write_u32(request.len() as u32).await.unwrap();
    stream.write_all(&request).await.unwrap();
    let response_length = stream.read_u32().await.unwrap();
    let mut response = vec![0; response_length as usize];
    stream.read_exact(response.as_mut_slice()).await.unwrap();
    let reply: LocalAgentIpcReply = serde_json::from_slice(&response).unwrap();
    assert_eq!(reply.request_id, "request-1");
    assert!(matches!(
        reply.response,
        LocalAgentIpcResponse::Runs { runs, .. } if runs.is_empty()
    ));

    cancellation.cancel();
    serving.await.unwrap().unwrap();
    assert!(!socket_path.exists());
}

#[tokio::test]
async fn rejects_a_peer_uid_mismatch_before_reading_a_request() {
    let directory = tempfile::tempdir().unwrap();
    let socket_path = directory.path().join("local-agent.sock");
    let expected_uid = unsafe { libc::geteuid() }.wrapping_add(1);
    let transport =
        UnixLocalAgentIpcTransport::bind(socket_path.clone(), server().await, expected_uid)
            .unwrap();
    let cancellation = CancellationToken::new();
    let serving = tokio::spawn(transport.serve(cancellation.clone()));
    let mut stream = UnixStream::connect(&socket_path).await.unwrap();
    let request = serde_json::to_vec(&list_request()).unwrap();
    stream.write_u32(request.len() as u32).await.unwrap();
    stream.write_all(&request).await.unwrap();
    let mut byte = [0u8; 1];
    let read = stream.read(&mut byte).await.unwrap_or(0);
    assert_eq!(read, 0);
    cancellation.cancel();
    serving.await.unwrap().unwrap();
}

#[tokio::test]
async fn refuses_relative_or_preexisting_socket_paths() {
    assert!(matches!(
        UnixLocalAgentIpcTransport::bind("relative.sock", server().await, unsafe {
            libc::geteuid()
        }),
        Err(UnixLocalAgentIpcError::InvalidSocketPath)
    ));
    let directory = tempfile::tempdir().unwrap();
    let socket_path = directory.path().join("occupied.sock");
    std::fs::write(&socket_path, b"do not replace").unwrap();
    assert!(matches!(
        UnixLocalAgentIpcTransport::bind(socket_path.clone(), server().await, unsafe {
            libc::geteuid()
        }),
        Err(UnixLocalAgentIpcError::SocketPathExists)
    ));
    assert_eq!(std::fs::read(socket_path).unwrap(), b"do not replace");
}
