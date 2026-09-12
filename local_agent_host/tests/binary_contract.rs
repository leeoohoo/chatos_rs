// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

#![cfg(target_os = "macos")]

use std::io::{Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::process::{Command, Stdio};

use chatos_local_agent_host::{
    LOCAL_AGENT_HOST_LAUNCH_PROTOCOL_VERSION, MAXIMUM_LOCAL_AGENT_LAUNCH_FRAME_BYTES,
};
use serde_json::json;

#[test]
fn bundled_binary_rejects_missing_secure_store_values_before_ready() {
    let directory = tempfile::tempdir().unwrap();
    let owner_user_id = format!("binary-contract-user-{}", std::process::id());
    let socket = directory.path().join("agent.sock");
    let database = directory.path().join("client.sqlite3");
    let grants = directory.path().join("attachment-grants");
    let platform_state = directory.path().join("platform-state");
    std::fs::create_dir(&grants).unwrap();
    std::fs::create_dir(&platform_state).unwrap();
    std::fs::set_permissions(&grants, std::fs::Permissions::from_mode(0o700)).unwrap();
    std::fs::set_permissions(&platform_state, std::fs::Permissions::from_mode(0o700)).unwrap();

    let request = json!({
        "protocol_version": LOCAL_AGENT_HOST_LAUNCH_PROTOCOL_VERSION,
        "launch_id": "binary-contract-launch",
        "owner_user_id": owner_user_id,
        "device_id": "binary-contract-device",
        "worker_id": "binary-contract-worker",
        "ipc_endpoint": { "transport": "unix_socket", "path": socket },
        "attachment_grant_directory": grants,
        "platform_state_directory": platform_state,
        "model_gateway_base_url": "https://gateway.example.test",
        "memory_engine_base_url": "https://memory.example.test",
        "memory_source_id": "binary-contract-source",
        "storage_profile": {
            "backend": "sqlite",
            "database_path": database,
            "encryption_secret": "binary-contract-sqlite-key",
        },
        "credential_references": {
            "model_access_token_reference": "missing-binary-contract-token",
            "provider_context_key_reference": "missing-binary-contract-provider-key",
        },
    });
    let body = serde_json::to_vec(&request).unwrap();
    assert!(body.len() <= MAXIMUM_LOCAL_AGENT_LAUNCH_FRAME_BYTES);

    let mut child = Command::new(env!("CARGO_BIN_EXE_chatos_local_agent_host"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    stdin
        .write_all(&u32::try_from(body.len()).unwrap().to_be_bytes())
        .unwrap();
    stdin.write_all(body.as_slice()).unwrap();
    stdin.flush().unwrap();
    drop(stdin);

    let mut output = Vec::new();
    child
        .stdout
        .take()
        .unwrap()
        .read_to_end(&mut output)
        .unwrap();
    assert!(!child.wait().unwrap().success());
    assert!(output.is_empty());
    assert!(!socket.exists());
}
