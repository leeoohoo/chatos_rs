// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

#![cfg(windows)]

use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use chatos_client_storage::{
    ClientStorage, RecordScope, SecretReference, SqliteBootstrapProfile, SqliteClientStorage,
    StorageEncryptionKey,
};
use chatos_local_agent_host::{
    LocalAgentIpcMutationExecutor, LocalAgentIpcServer, WindowsLocalAgentIpcError,
    WindowsLocalAgentIpcTransport,
};
use chatos_local_agent_protocol::{
    LocalAgentCommand, LocalAgentIpcError, LocalAgentIpcReply, LocalAgentIpcRequest,
    LocalAgentIpcResponse, LOCAL_AGENT_PROTOCOL_VERSION,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::windows::named_pipe::ClientOptions;
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
                encryption_secret: SecretReference::new("test:windows-ipc-key").unwrap(),
            },
            &StorageEncryptionKey::new([92; 32]),
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

fn unique_pipe_name() -> String {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    format!(
        r"\\.\pipe\chatos-local-agent-test-{}-{nonce}",
        std::process::id()
    )
}

#[tokio::test]
async fn serves_one_length_delimited_request_to_the_same_windows_user() {
    let pipe_name = unique_pipe_name();
    let transport = WindowsLocalAgentIpcTransport::bind(pipe_name.clone(), server().await).unwrap();
    let cancellation = CancellationToken::new();
    let serving = tokio::spawn(transport.serve(cancellation.clone()));

    let mut client = ClientOptions::new().open(pipe_name).unwrap();
    let request = serde_json::to_vec(&list_request()).unwrap();
    client.write_u32(request.len() as u32).await.unwrap();
    client.write_all(&request).await.unwrap();
    let response_length = client.read_u32().await.unwrap();
    let mut response = vec![0; response_length as usize];
    client.read_exact(response.as_mut_slice()).await.unwrap();
    let reply: LocalAgentIpcReply = serde_json::from_slice(&response).unwrap();
    assert_eq!(reply.request_id, "request-1");
    assert!(matches!(
        reply.response,
        LocalAgentIpcResponse::Runs { runs, .. } if runs.is_empty()
    ));

    cancellation.cancel();
    serving.await.unwrap().unwrap();
}

#[tokio::test]
async fn rejects_pipe_names_outside_the_private_namespace() {
    assert!(matches!(
        WindowsLocalAgentIpcTransport::bind(r"\\.\pipe\some-other-service", server().await,),
        Err(WindowsLocalAgentIpcError::InvalidPipeName)
    ));
}
