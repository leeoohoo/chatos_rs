// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

#![cfg(unix)]

use std::io::{Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::process::{Command, Stdio};

use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use chatos_local_agent_host::{
    LocalAgentHostReady, LOCAL_AGENT_HOST_LAUNCH_PROTOCOL_VERSION,
    MAXIMUM_LOCAL_AGENT_LAUNCH_FRAME_BYTES,
};
use serde_json::json;

#[test]
fn bundled_binary_performs_ready_handshake_and_shuts_down_on_sigterm() {
    let directory = tempfile::tempdir().unwrap();
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
        "owner_user_id": "binary-contract-user",
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
        "credentials": {
            "model_access_token": "binary-contract-access-token",
            "provider_context_key_base64": STANDARD.encode([3_u8; 32]),
            "storage": {
                "backend": "sqlite",
                "encryption_secret_reference": "binary-contract-sqlite-key",
                "encryption_key_base64": STANDARD.encode([7_u8; 32]),
            },
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

    let mut stdout = child.stdout.take().unwrap();
    let mut length = [0_u8; 4];
    stdout.read_exact(&mut length).unwrap();
    let length = u32::from_be_bytes(length) as usize;
    assert!((1..=MAXIMUM_LOCAL_AGENT_LAUNCH_FRAME_BYTES).contains(&length));
    let mut ready_body = vec![0_u8; length];
    stdout.read_exact(ready_body.as_mut_slice()).unwrap();
    let ready: LocalAgentHostReady = serde_json::from_slice(&ready_body).unwrap();
    assert_eq!(
        ready.protocol_version,
        LOCAL_AGENT_HOST_LAUNCH_PROTOCOL_VERSION
    );
    assert_eq!(ready.launch_id, "binary-contract-launch");
    assert_eq!(ready.process_id, child.id());
    assert_eq!(ready.client_endpoint, socket.to_string_lossy());
    assert!(socket.exists());

    // SAFETY: `child.id()` is the live process created above and SIGTERM is
    // used to verify the executable's graceful shutdown contract.
    assert_eq!(unsafe { libc::kill(child.id() as i32, libc::SIGTERM) }, 0);
    assert!(child.wait().unwrap().success());
    assert!(!socket.exists());
}
