// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use chatos_client_storage::{SecretReference, StorageSecretResolver};
use chatos_local_agent_host::{
    read_local_agent_host_launch_request, write_local_agent_host_ready,
    LocalAgentHostBootstrapError, LocalAgentHostReady, LocalAgentLaunchStorageCredentials,
    LOCAL_AGENT_HOST_LAUNCH_PROTOCOL_VERSION, MAXIMUM_LOCAL_AGENT_LAUNCH_FRAME_BYTES,
};
use serde_json::{json, Value};
use std::io::Cursor;

fn sqlite_launch(
    socket_path: &str,
    attachment_grant_directory: &str,
    platform_state_directory: &str,
) -> Value {
    json!({
        "protocol_version": LOCAL_AGENT_HOST_LAUNCH_PROTOCOL_VERSION,
        "launch_id": "launch-1",
        "owner_user_id": "user-1",
        "device_id": "device-1",
        "worker_id": "worker-1",
        "ipc_endpoint": {
            "transport": "unix_socket",
            "path": socket_path,
        },
        "attachment_grant_directory": attachment_grant_directory,
        "platform_state_directory": platform_state_directory,
        "model_gateway_base_url": "https://api.example.com",
        "memory_engine_base_url": "https://memory.example.com",
        "memory_source_id": "local-agent",
        "storage_profile": {
            "backend": "sqlite",
            "database_path": format!("{socket_path}.sqlite"),
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
    })
}

fn framed(value: &Value) -> Cursor<Vec<u8>> {
    let body = serde_json::to_vec(value).unwrap();
    let mut bytes = Vec::with_capacity(body.len() + 4);
    bytes.extend_from_slice(&(body.len() as u32).to_be_bytes());
    bytes.extend_from_slice(body.as_slice());
    Cursor::new(bytes)
}

#[tokio::test]
async fn reads_one_strict_length_prefixed_launch_request() {
    let directory = tempfile::tempdir().unwrap();
    let socket = directory.path().join("agent.sock");
    let grants = private_grant_directory(directory.path());
    let state = private_state_directory(directory.path());
    let mut input = framed(&sqlite_launch(
        socket.to_str().unwrap(),
        grants.to_str().unwrap(),
        state.to_str().unwrap(),
    ));

    let request = read_local_agent_host_launch_request(&mut input)
        .await
        .unwrap();

    assert_eq!(request.owner_user_id, "user-1");
    assert_eq!(request.provider_context_key().unwrap(), [3_u8; 32]);
    assert_eq!(
        request.ipc_endpoint.client_endpoint(),
        socket.to_str().unwrap()
    );
    let LocalAgentLaunchStorageCredentials::Sqlite {
        encryption_key_base64,
        ..
    } = &request.credentials.storage
    else {
        panic!("expected SQLite credentials");
    };
    assert_eq!(
        STANDARD.decode(encryption_key_base64.expose()).unwrap(),
        [7_u8; 32]
    );
    request
        .credentials
        .resolve_sqlite_encryption_key(&SecretReference::new("sqlite-secret-1").unwrap())
        .await
        .unwrap();
    assert!(request
        .credentials
        .resolve_sqlite_encryption_key(&SecretReference::new("wrong-secret").unwrap())
        .await
        .is_err());
}

#[tokio::test]
async fn rejects_unknown_fields_and_profile_credential_mismatches() {
    let directory = tempfile::tempdir().unwrap();
    let socket = directory.path().join("agent.sock");
    let grants = private_grant_directory(directory.path());
    let state = private_state_directory(directory.path());
    let mut unknown = sqlite_launch(
        socket.to_str().unwrap(),
        grants.to_str().unwrap(),
        state.to_str().unwrap(),
    );
    unknown
        .as_object_mut()
        .unwrap()
        .insert("legacy_fallback".to_string(), json!(true));
    assert_eq!(
        read_local_agent_host_launch_request(&mut framed(&unknown))
            .await
            .unwrap_err(),
        LocalAgentHostBootstrapError::InvalidJson
    );

    let mut mismatch = sqlite_launch(
        socket.to_str().unwrap(),
        grants.to_str().unwrap(),
        state.to_str().unwrap(),
    );
    mismatch["credentials"]["storage"]["encryption_secret_reference"] = json!("another-secret");
    assert_eq!(
        read_local_agent_host_launch_request(&mut framed(&mismatch))
            .await
            .unwrap_err(),
        LocalAgentHostBootstrapError::StorageCredentialMismatch
    );
}

#[tokio::test]
async fn rejects_remote_postgres_without_verified_tls() {
    let directory = tempfile::tempdir().unwrap();
    let socket = directory.path().join("agent.sock");
    let grants = private_grant_directory(directory.path());
    let state = private_state_directory(directory.path());
    let mut request = sqlite_launch(
        socket.to_str().unwrap(),
        grants.to_str().unwrap(),
        state.to_str().unwrap(),
    );
    request["storage_profile"] = json!({
        "backend": "postgres",
        "connection_secret": "postgres-secret-1",
    });
    request["credentials"]["storage"] = json!({
        "backend": "postgres",
        "connection_secret_reference": "postgres-secret-1",
        "host": "database.example.com",
        "port": 5432,
        "database": "chatos",
        "tls_mode": "disabled",
        "username": "chatos",
        "password": "private-password",
    });

    assert_eq!(
        read_local_agent_host_launch_request(&mut framed(&request))
            .await
            .unwrap_err(),
        LocalAgentHostBootstrapError::InvalidCredential("postgres_tls_mode")
    );
}

#[tokio::test]
async fn never_renders_launch_credentials_in_debug_output() {
    let directory = tempfile::tempdir().unwrap();
    let socket = directory.path().join("agent.sock");
    let grants = private_grant_directory(directory.path());
    let state = private_state_directory(directory.path());
    let request = read_local_agent_host_launch_request(&mut framed(&sqlite_launch(
        socket.to_str().unwrap(),
        grants.to_str().unwrap(),
        state.to_str().unwrap(),
    )))
    .await
    .unwrap();

    let rendered = format!("{request:?}");

    assert!(!rendered.contains("private-model-token"));
    assert!(!rendered.contains(STANDARD.encode([3_u8; 32]).as_str()));
    assert!(!rendered.contains(STANDARD.encode([7_u8; 32]).as_str()));
    assert!(!rendered.contains(grants.to_str().unwrap()));
    assert!(rendered.contains("[PRIVATE DIRECTORY]"));
    assert!(rendered.contains("[REDACTED]"));
}

fn private_grant_directory(parent: &std::path::Path) -> std::path::PathBuf {
    let path = parent.join("attachment-grants");
    std::fs::create_dir(&path).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700)).unwrap();
    }
    path
}

fn private_state_directory(parent: &std::path::Path) -> std::path::PathBuf {
    let path = parent.join("platform-state");
    std::fs::create_dir(&path).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700)).unwrap();
    }
    path
}

#[tokio::test]
async fn rejects_oversized_and_truncated_launch_frames() {
    let mut oversized = Cursor::new(
        ((MAXIMUM_LOCAL_AGENT_LAUNCH_FRAME_BYTES + 1) as u32)
            .to_be_bytes()
            .to_vec(),
    );
    assert_eq!(
        read_local_agent_host_launch_request(&mut oversized)
            .await
            .unwrap_err(),
        LocalAgentHostBootstrapError::InvalidFrameSize
    );

    let mut truncated = Cursor::new([5_u32.to_be_bytes().as_slice(), &[1, 2]].concat());
    assert_eq!(
        read_local_agent_host_launch_request(&mut truncated)
            .await
            .unwrap_err(),
        LocalAgentHostBootstrapError::TruncatedFrame
    );
}

#[tokio::test]
async fn writes_only_a_correlated_non_secret_ready_frame() {
    let ready = LocalAgentHostReady {
        protocol_version: LOCAL_AGENT_HOST_LAUNCH_PROTOCOL_VERSION,
        launch_id: "launch-1".to_string(),
        process_id: 42,
        client_endpoint: "chatos-local-agent-7bb214f0".to_string(),
    };
    let mut output = Cursor::new(Vec::new());

    write_local_agent_host_ready(&mut output, &ready)
        .await
        .unwrap();

    let bytes = output.into_inner();
    let length = u32::from_be_bytes(bytes[..4].try_into().unwrap()) as usize;
    assert_eq!(length, bytes.len() - 4);
    let decoded: LocalAgentHostReady = serde_json::from_slice(&bytes[4..]).unwrap();
    assert_eq!(decoded, ready);
    let text = String::from_utf8(bytes[4..].to_vec()).unwrap();
    assert!(!text.contains("token"));
    assert!(!text.contains("password"));
    assert!(!text.contains("key"));
}
