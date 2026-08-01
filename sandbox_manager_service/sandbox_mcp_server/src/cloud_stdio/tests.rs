// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

fn service() -> (CloudStdioService, tempfile::TempDir) {
    let temp = tempfile::tempdir().unwrap();
    let workspace = temp.path().join("workspace");
    let state_dir = temp.path().join("state");
    std::fs::create_dir_all(workspace.as_path()).unwrap();
    std::fs::create_dir_all(state_dir.as_path()).unwrap();
    let config = ServerConfig {
        host: "127.0.0.1".to_string(),
        port: 0,
        workspace,
        state_dir,
        auth_token: Some("secret".to_string()),
        project_id: Some("project-1".to_string()),
        user_id: Some("user-1".to_string()),
        max_file_bytes: 1024,
        max_write_bytes: 1024,
        search_limit: 10,
        terminal_idle_timeout_ms: 1_000,
        terminal_max_wait_ms: 1_000,
        terminal_max_output_chars: 1_000,
        disk_limit_bytes: None,
        extra_quota_roots: Vec::new(),
        permission_profile: "workspace_write".to_string(),
        command_sandbox_backend: "external".to_string(),
        additional_writable_roots: Vec::new(),
        host_home: None,
        effective_permissions: None,
    };
    (CloudStdioService::new(config), temp)
}

fn request(command: &str, args: Vec<String>) -> CloudStdioCallRequest {
    CloudStdioCallRequest {
        runtime_session_id: "mcp_session_1".to_string(),
        resource_id: "resource-1".to_string(),
        invocation_id: None,
        command: command.to_string(),
        args,
        env: BTreeMap::new(),
        cwd: None,
        plugin_artifact: None,
        plugin_workspace_write: false,
        method: "tools/list".to_string(),
        params: serde_json::json!({}),
        expires_at_unix: chrono::Utc::now().timestamp() + 60,
        timeout_ms: 5_000,
    }
}

#[test]
fn command_rejects_absolute_paths_and_shell_eval() {
    assert!(validate_command("/usr/bin/node", &[]).is_err());
    assert!(validate_command("bash", &["-c".to_string(), "echo bad".to_string()]).is_err());
    assert!(validate_command("npx", &["-y".to_string(), "@example/mcp".to_string()]).is_ok());
}

#[test]
fn environment_rejects_host_controlled_names() {
    for name in [
        "PATH",
        "CHATOS_SANDBOX_MCP_TOKEN",
        "LD_PRELOAD",
        "MCP_MANAGEMENT_INTERNAL_API_SECRET",
    ] {
        assert!(
            validate_environment(&BTreeMap::from([(name.to_string(), "secret".to_string(),)]))
                .is_err()
        );
    }
    assert!(validate_environment(&BTreeMap::from([(
        "GITHUB_TOKEN".to_string(),
        "secret".to_string(),
    )]))
    .is_ok());
}

#[test]
fn cwd_rejects_parent_traversal_and_symlink_escape() {
    let temp = tempfile::tempdir().unwrap();
    let workspace = temp.path().join("workspace");
    std::fs::create_dir_all(workspace.join("nested")).unwrap();
    let workspace = workspace.canonicalize().unwrap();
    assert_eq!(
        resolve_workspace_cwd(workspace.as_path(), Some("nested")).unwrap(),
        workspace.join("nested")
    );
    assert!(resolve_workspace_cwd(workspace.as_path(), Some("../outside")).is_err());
}

#[tokio::test]
async fn active_session_rejects_runtime_binding_drift_and_can_close() {
    let (service, _temp) = service();
    let first_request = request("npx", vec!["-y".to_string()]);
    let first = service
        .prepare_binding(
            &first_request,
            binding_request_fingerprint(&first_request).unwrap(),
        )
        .await
        .unwrap();
    assert!(service.register_binding(&first).await.unwrap());
    assert!(first.launch_spec_path.is_file());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(first.launch_spec_path.as_path())
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o077, 0);
    }
    assert!(!service.register_binding(&first).await.unwrap());

    let changed_request = request("node", vec!["server.js".to_string()]);
    let changed = service
        .prepare_binding(
            &changed_request,
            binding_request_fingerprint(&changed_request).unwrap(),
        )
        .await
        .unwrap();
    assert!(service.register_binding(&changed).await.is_err());
    let closed = service
        .close(CloudStdioCloseRequest {
            runtime_session_id: "mcp_session_1".to_string(),
            resource_id: "resource-1".to_string(),
        })
        .await
        .unwrap();
    assert!(closed.closed);
    assert!(!first.launch_spec_path.exists());
}

#[tokio::test]
async fn cancellation_is_scoped_to_the_exact_active_invocation() {
    let (service, _temp) = service();
    let mut call = request("node", vec!["server.js".to_string()]);
    call.method = "tools/call".to_string();
    call.params = serde_json::json!({"name": "slow", "arguments": {}});
    call.invocation_id = Some("invocation-1".to_string());
    let config =
        McpStdioServer::new("test-cancel", "node").with_user_id("mcp_session_1:resource-1");
    let active = service
        .register_invocation(&call, "mcp_session_1:resource-1", &config)
        .await
        .unwrap()
        .unwrap();
    let watcher = active.clone();
    let acknowledgement = tokio::spawn(async move {
        watcher.cancellation.cancelled().await;
        mark_invocation_state(&watcher, INVOCATION_CANCELLED);
    });

    let response = service
        .cancel(CloudStdioCancelRequest {
            runtime_session_id: "mcp_session_1".to_string(),
            resource_id: "resource-1".to_string(),
            invocation_id: "invocation-1".to_string(),
        })
        .await
        .unwrap();
    assert_eq!(response.status, "cancelled");
    acknowledgement.await.unwrap();

    let missing = service
        .cancel(CloudStdioCancelRequest {
            runtime_session_id: "mcp_session_1".to_string(),
            resource_id: "resource-1".to_string(),
            invocation_id: "invocation-1".to_string(),
        })
        .await
        .unwrap();
    assert_eq!(missing.status, "invocation_not_found");
}

#[tokio::test]
async fn tools_call_requires_a_bounded_invocation_id() {
    let (service, _temp) = service();
    let mut call = request("node", vec!["server.js".to_string()]);
    call.method = "tools/call".to_string();
    call.params = serde_json::json!({"name": "slow", "arguments": {}});
    let config = McpStdioServer::new("test-cancel-id", "node");
    assert!(service
        .register_invocation(&call, "mcp_session_1:resource-1", &config)
        .await
        .is_err());
    call.invocation_id = Some("bad invocation".to_string());
    assert!(service
        .register_invocation(&call, "mcp_session_1:resource-1", &config)
        .await
        .is_err());
}
