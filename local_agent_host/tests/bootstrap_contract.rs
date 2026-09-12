// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_local_agent_host::{
    read_local_agent_host_launch_request, write_local_agent_host_ready,
    LocalAgentHostBootstrapError, LocalAgentHostReady, LOCAL_AGENT_HOST_LAUNCH_PROTOCOL_VERSION,
    MAXIMUM_LOCAL_AGENT_LAUNCH_FRAME_BYTES,
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
        "credential_references": {
            "model_access_token_reference": "model-access-token",
            "provider_context_key_reference": "provider-context-key",
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
    assert_eq!(
        request.ipc_endpoint.client_endpoint(),
        socket.to_str().unwrap()
    );
    assert_eq!(
        request.credential_references.model_access_token_reference,
        "model-access-token"
    );
    assert_eq!(
        request.credential_references.provider_context_key_reference,
        "provider-context-key"
    );
}

#[tokio::test]
async fn rejects_unknown_fields_and_raw_credential_material() {
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

    let mut secret_bearing = sqlite_launch(
        socket.to_str().unwrap(),
        grants.to_str().unwrap(),
        state.to_str().unwrap(),
    );
    secret_bearing.as_object_mut().unwrap().insert(
        "credentials".to_string(),
        json!({"model_access_token": "must-never-cross-launch-pipe"}),
    );
    assert_eq!(
        read_local_agent_host_launch_request(&mut framed(&secret_bearing))
            .await
            .unwrap_err(),
        LocalAgentHostBootstrapError::InvalidJson
    );
}

#[tokio::test]
async fn rejects_invalid_credential_references() {
    let directory = tempfile::tempdir().unwrap();
    let socket = directory.path().join("agent.sock");
    let grants = private_grant_directory(directory.path());
    let state = private_state_directory(directory.path());
    let mut request = sqlite_launch(
        socket.to_str().unwrap(),
        grants.to_str().unwrap(),
        state.to_str().unwrap(),
    );
    request["credential_references"]["model_access_token_reference"] = json!(" token-ref");

    assert_eq!(
        read_local_agent_host_launch_request(&mut framed(&request))
            .await
            .unwrap_err(),
        LocalAgentHostBootstrapError::InvalidIdentity("model_access_token_reference")
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
    assert!(!rendered.contains("must-never-cross-launch-pipe"));
    assert!(!rendered.contains(grants.to_str().unwrap()));
    assert!(rendered.contains("[PRIVATE DIRECTORY]"));
    assert!(rendered.contains("model-access-token"));
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
