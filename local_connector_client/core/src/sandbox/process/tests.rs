// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;
use chatos_sandbox_contract::{
    legacy_policy_permission_snapshot, ApprovalPolicy, ApprovalReviewer, NetworkDomainPermission,
    NetworkPermissionPolicy, NetworkRequirements, PermissionProfileId, SandboxBackendKind,
};
use std::collections::BTreeMap;

fn rpc(id: &str, method: &str, params: Value) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": method,
        "params": params,
    })
}

#[tokio::test]
#[ignore = "requires `cargo build -p chatos_sandbox_mcp_server` before this test"]
async fn native_agent_enforces_seatbelt_network_filesystem_and_process_tree() {
    native_sandbox_agent_executable().expect("sandbox agent binary");
    let root =
        std::env::temp_dir().join(format!("chatos-native-agent-test-{}", uuid::Uuid::new_v4()));
    let workspace = root.join("workspace");
    let outside = root.join("outside");
    std::fs::create_dir_all(workspace.join(".git")).expect("workspace");
    std::fs::create_dir_all(&outside).expect("outside");
    std::fs::write(outside.join("secret.txt"), "outside-secret").expect("outside secret");
    std::os::unix::fs::symlink(&outside, workspace.join("escape")).expect("escape symlink");

    let runtime = LocalSandboxRuntime::default();
    let sandbox_id = format!("sandbox-test-{}", uuid::Uuid::new_v4());
    let policy = EffectiveSandboxPolicy {
        sandbox_mode: SandboxBackendKind::LocalProcess,
        permission_profile_id: PermissionProfileId::WorkspaceWrite,
        approval_policy: ApprovalPolicy::OnRequest,
        approval_reviewer: ApprovalReviewer::User,
        policy_revision: None,
        additional_writable_roots: Vec::new(),
    };
    let limits = LocalSandboxResourceLimits {
        cpu: 1.0,
        memory_mb: 512,
        disk_mb: 128,
        max_processes: 32,
    };
    let effective_permissions =
        legacy_policy_permission_snapshot(&policy, vec![workspace.to_string_lossy().to_string()]);
    start_native_sandbox_process(
        &runtime,
        sandbox_id.as_str(),
        workspace.as_path(),
        &policy,
        &effective_permissions,
        &limits,
        "project-test",
        "user-test",
    )
    .await
    .expect("start native sandbox");

    let tools = call_native_sandbox_mcp(
        &runtime,
        sandbox_id.as_str(),
        &rpc("tools", "tools/list", json!({})),
    )
    .await
    .expect("list tools");
    assert!(tools.to_string().contains("execute_command"));

    let symlink_read = call_native_sandbox_mcp(
        &runtime,
        sandbox_id.as_str(),
        &rpc(
            "symlink-read",
            "tools/call",
            json!({
                "name": "read_file",
                "arguments": { "path": "escape/secret.txt" },
            }),
        ),
    )
    .await
    .expect("symlink read response");
    assert!(
        !symlink_read.to_string().contains("outside-secret"),
        "workspace-scoped file tool followed a symlink outside the workspace: {symlink_read}"
    );

    let command = format!(
        "touch '{}' && ! touch '{}' && ! touch '{}'",
        workspace.join("inside").display(),
        outside.join("blocked").display(),
        workspace.join(".git/blocked").display()
    );
    let response = call_native_sandbox_mcp(
        &runtime,
        sandbox_id.as_str(),
        &rpc(
            "filesystem",
            "tools/call",
            json!({
                "name": "execute_command",
                "arguments": { "path": ".", "command": command },
            }),
        ),
    )
    .await
    .expect("filesystem probe");
    assert!(response.get("error").is_none(), "{response}");
    assert!(workspace.join("inside").exists(), "{response}");
    assert!(!outside.join("blocked").exists());
    assert!(!workspace.join(".git/blocked").exists());

    let outside_permission = json!({
        "fileSystem": {
            "entries": [{
                "access": "write",
                "path": {
                    "type": "path",
                    "path": outside.to_string_lossy()
                }
            }]
        }
    });
    let elevated_response = call_native_sandbox_mcp(
        &runtime,
        sandbox_id.as_str(),
        &rpc(
            "filesystem-elevated",
            "tools/call",
            json!({
                "name": "execute_command",
                "arguments": {
                    "path": ".",
                    "command": format!("touch '{}'", outside.join("allowed-once").display()),
                    "additionalPermissions": outside_permission,
                    "_grantedPermissions": outside_permission,
                },
            }),
        ),
    )
    .await
    .expect("elevated filesystem probe");
    assert!(
        elevated_response.get("error").is_none(),
        "{elevated_response}"
    );
    assert!(outside.join("allowed-once").exists());

    let post_elevation_response = call_native_sandbox_mcp(
        &runtime,
        sandbox_id.as_str(),
        &rpc(
            "filesystem-post-elevation",
            "tools/call",
            json!({
                "name": "execute_command",
                "arguments": {
                    "path": ".",
                    "command": format!("! touch '{}'", outside.join("must-stay-blocked").display()),
                },
            }),
        ),
    )
    .await
    .expect("post elevation filesystem probe");
    assert!(
        post_elevation_response.get("error").is_none(),
        "{post_elevation_response}"
    );
    assert!(!outside.join("must-stay-blocked").exists());

    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("listener");
    listener.set_nonblocking(true).expect("nonblocking");
    let port = listener.local_addr().expect("listener address").port();
    let response = call_native_sandbox_mcp(
        &runtime,
        sandbox_id.as_str(),
        &rpc(
            "network",
            "tools/call",
            json!({
                "name": "execute_command",
                "arguments": {
                    "path": ".",
                    "command": format!("if /usr/bin/nc -z 127.0.0.1 {port}; then exit 42; else exit 0; fi")
                },
            }),
        ),
    )
    .await
    .expect("network probe");
    assert!(response.get("error").is_none(), "{response}");
    assert!(
        listener.accept().is_err(),
        "sandbox unexpectedly reached loopback"
    );

    let network_permission = json!({ "network": { "enabled": true } });
    let response = call_native_sandbox_mcp(
        &runtime,
        sandbox_id.as_str(),
        &rpc(
            "network-elevated",
            "tools/call",
            json!({
                "name": "execute_command",
                "arguments": {
                    "path": ".",
                    "command": format!("/usr/bin/nc -z 127.0.0.1 {port}"),
                    "additionalPermissions": network_permission,
                    "_grantedPermissions": network_permission,
                },
            }),
        ),
    )
    .await
    .expect("elevated network probe");
    assert!(response.get("error").is_none(), "{response}");
    assert!(
        listener.accept().is_ok(),
        "approved command did not receive network access"
    );

    let child_pid_path = workspace.join("child.pid");
    let child_survival_marker = workspace.join("child-survived");
    call_native_sandbox_mcp(
        &runtime,
        sandbox_id.as_str(),
        &rpc(
            "background",
            "tools/call",
            json!({
                "name": "execute_command",
                "arguments": {
                    "path": ".",
                    "command": format!(
                        "(sleep 2; touch '{}') & echo $! > '{}'; wait",
                        child_survival_marker.display(),
                        child_pid_path.display()
                    ),
                    "background": true
                },
            }),
        ),
    )
    .await
    .expect("background process");
    for _ in 0..50 {
        if child_pid_path.exists() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let child_pid: i32 = std::fs::read_to_string(&child_pid_path)
        .expect("child pid file")
        .trim()
        .parse()
        .expect("child pid");
    #[cfg(target_os = "linux")]
    let _ = child_pid;
    #[cfg(target_os = "macos")]
    assert_eq!(unsafe { libc::kill(child_pid, 0) }, 0);

    destroy_native_sandbox_process(&runtime, sandbox_id.as_str())
        .await
        .expect("destroy native sandbox");
    #[cfg(target_os = "macos")]
    {
        for _ in 0..50 {
            if unsafe { libc::kill(child_pid, 0) } == -1 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        assert_eq!(unsafe { libc::kill(child_pid, 0) }, -1);
    }
    tokio::time::sleep(Duration::from_millis(2_300)).await;
    assert!(
        !child_survival_marker.exists(),
        "descendant survived normal sandbox release"
    );

    let crash_runtime = LocalSandboxRuntime::default();
    let crash_sandbox_id = format!("sandbox-crash-test-{}", uuid::Uuid::new_v4());
    start_native_sandbox_process(
        &crash_runtime,
        crash_sandbox_id.as_str(),
        workspace.as_path(),
        &policy,
        &effective_permissions,
        &limits,
        "project-test",
        "user-test",
    )
    .await
    .expect("start crash sandbox");
    let crash_child_pid_path = workspace.join("crash-child.pid");
    let crash_survival_marker = workspace.join("crash-child-survived");
    call_native_sandbox_mcp(
        &crash_runtime,
        crash_sandbox_id.as_str(),
        &rpc(
            "crash-background",
            "tools/call",
            json!({
                "name": "execute_command",
                "arguments": {
                    "path": ".",
                    "command": format!(
                        "(sleep 2; touch '{}') & echo $! > '{}'; wait",
                        crash_survival_marker.display(),
                        crash_child_pid_path.display()
                    ),
                    "background": true
                },
            }),
        ),
    )
    .await
    .expect("crash background process");
    for _ in 0..50 {
        if crash_child_pid_path.exists() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let crash_child_pid: i32 = std::fs::read_to_string(&crash_child_pid_path)
        .expect("crash child pid file")
        .trim()
        .parse()
        .expect("crash child pid");
    #[cfg(target_os = "linux")]
    let _ = crash_child_pid;
    let crash_state_root = crash_runtime
        .processes
        .read()
        .await
        .get(crash_sandbox_id.as_str())
        .expect("crash process")
        .state_root
        .clone();
    drop(crash_runtime);
    #[cfg(target_os = "macos")]
    {
        for _ in 0..100 {
            if unsafe { libc::kill(crash_child_pid, 0) } == -1 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        assert_eq!(unsafe { libc::kill(crash_child_pid, 0) }, -1);
    }
    tokio::time::sleep(Duration::from_millis(2_300)).await;
    assert!(
        !crash_survival_marker.exists(),
        "descendant survived broker stdio/process-group teardown"
    );
    let _ = std::fs::remove_dir_all(crash_state_root);
    let _ = std::fs::remove_dir_all(root);
}

#[tokio::test]
#[ignore = "requires `cargo build --release -p chatos_sandbox_mcp_server` before this test"]
async fn native_agent_applies_effective_domain_network_snapshot() {
    native_sandbox_agent_executable().expect("sandbox agent binary");
    let root = std::env::temp_dir().join(format!(
        "chatos-native-network-agent-test-{}",
        uuid::Uuid::new_v4()
    ));
    let workspace = root.join("workspace");
    std::fs::create_dir_all(&workspace).expect("workspace");

    let policy = EffectiveSandboxPolicy {
        sandbox_mode: SandboxBackendKind::LocalProcess,
        permission_profile_id: PermissionProfileId::WorkspaceWrite,
        approval_policy: ApprovalPolicy::OnRequest,
        approval_reviewer: ApprovalReviewer::User,
        policy_revision: Some("network-test".to_string()),
        additional_writable_roots: Vec::new(),
    };
    let mut effective_permissions =
        legacy_policy_permission_snapshot(&policy, vec![workspace.to_string_lossy().to_string()]);
    effective_permissions.network = NetworkPermissionPolicy::Restricted {
        requirements: NetworkRequirements {
            enabled: Some(true),
            domains: Some(BTreeMap::from([(
                "127.0.0.1".to_string(),
                NetworkDomainPermission::Allow,
            )])),
            enable_socks5: Some(false),
            ..Default::default()
        },
    };

    let allowed_listener = std::net::TcpListener::bind("127.0.0.1:0").expect("listener");
    let allowed_port = allowed_listener.local_addr().expect("address").port();
    let allowed_server = std::thread::spawn(move || {
        let (mut stream, _) = allowed_listener.accept().expect("accept proxied request");
        use std::io::{Read, Write};
        let mut request = [0_u8; 4096];
        let _ = stream.read(&mut request).expect("read request");
        stream
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 7\r\nConnection: close\r\n\r\nproxied")
            .expect("write response");
    });

    let bypass_listener = std::net::TcpListener::bind("127.0.0.1:0").expect("listener");
    bypass_listener
        .set_nonblocking(true)
        .expect("nonblocking listener");
    let bypass_port = bypass_listener.local_addr().expect("address").port();

    let runtime = LocalSandboxRuntime::default();
    let sandbox_id = format!("sandbox-network-test-{}", uuid::Uuid::new_v4());
    start_native_sandbox_process(
        &runtime,
        sandbox_id.as_str(),
        workspace.as_path(),
        &policy,
        &effective_permissions,
        &LocalSandboxResourceLimits {
            cpu: 1.0,
            memory_mb: 512,
            disk_mb: 128,
            max_processes: 32,
        },
        "project-test",
        "user-test",
    )
    .await
    .expect("start native sandbox");

    let proxied = call_native_sandbox_mcp(
        &runtime,
        sandbox_id.as_str(),
        &rpc(
            "network-proxy",
            "tools/call",
            json!({
                "name": "execute_command",
                "arguments": {
                    "path": ".",
                    "command": format!(
                        "/usr/bin/curl --silent --show-error --max-time 3 http://127.0.0.1:{allowed_port}/"
                    )
                },
            }),
        ),
    )
    .await
    .expect("proxied request");
    assert!(proxied.get("error").is_none(), "{proxied}");
    assert!(proxied.to_string().contains("proxied"), "{proxied}");
    allowed_server.join().expect("allowed server");

    let bypass = call_native_sandbox_mcp(
        &runtime,
        sandbox_id.as_str(),
        &rpc(
            "network-bypass",
            "tools/call",
            json!({
                "name": "execute_command",
                "arguments": {
                    "path": ".",
                    "command": format!(
                        "if /usr/bin/curl --noproxy '*' --silent --show-error --max-time 1 http://127.0.0.1:{bypass_port}/; then exit 42; else exit 0; fi"
                    )
                },
            }),
        ),
    )
    .await
    .expect("bypass request");
    assert!(bypass.get("error").is_none(), "{bypass}");
    assert!(
        bypass_listener.accept().is_err(),
        "command bypassed the managed proxy"
    );

    destroy_native_sandbox_process(&runtime, sandbox_id.as_str())
        .await
        .expect("destroy sandbox");
    let _ = std::fs::remove_dir_all(root);
}
