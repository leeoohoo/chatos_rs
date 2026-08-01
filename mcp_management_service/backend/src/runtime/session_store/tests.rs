// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_mcp_management_sdk::{
    ExecutionPlane, ProjectExecutionContext, SandboxProviderKind, WorkspaceProviderKind,
};
use chatos_plugin_management_sdk::{PluginExecutionHost, PluginMcpServer};

use super::*;

fn plugin_runtime_binding() -> PluginMcpRuntimeBinding {
    PluginMcpRuntimeBinding {
        provider_ref: format!("plugin-binding:{}", "b".repeat(64)),
        resource_id: "plugin-mcp-1".to_string(),
        plugin_id: "private-plugin-1".to_string(),
        release_id: "private-release-1".to_string(),
        version: "1.0.0".to_string(),
        artifact_sha256: "a".repeat(64),
        normalized_manifest_sha256: "b".repeat(64),
        component_key: "workspace".to_string(),
        component_content_sha256: "c".repeat(64),
        declared_execution_host: PluginExecutionHost::Local,
        installation_device_id: Some("device-private-1".to_string()),
        permission_snapshot: vec!["workspace.read".to_string()],
        auth_connection_ids: vec!["oauth-private-reference".to_string()],
        runtime: PluginMcpServer::Http {
            component_key: "workspace".to_string(),
            url: "https://plugin-private.example.com/mcp".to_string(),
            headers: Default::default(),
            oauth_resource: None,
            connect_timeout_ms: None,
        },
        server_key: None,
        tool_allowlist: vec!["read_file".to_string()],
        tool_blocklist: Vec::new(),
        required: true,
        allow_writes: false,
    }
}

fn snapshot(session_id: &str) -> RuntimeSessionSnapshot {
    let expires_at_unix = chrono::Utc::now().timestamp() + 300;
    let plugin_runtime = plugin_runtime_binding();
    let mut headers = HeaderMap::new();
    let mut authorization = HeaderValue::from_static("Bearer shared-store-secret");
    authorization.set_sensitive(true);
    headers.insert("authorization", authorization);
    RuntimeSessionSnapshot {
        session_id: session_id.to_string(),
        caller_service: "task-runner".to_string(),
        owner_user_id: "owner-1".to_string(),
        agent_key: "task_runner_run_phase".to_string(),
        project_id: "project-1".to_string(),
        run_id: Some("run-1".to_string()),
        turn_id: Some("turn-1".to_string()),
        task_id: Some("task-1".to_string()),
        source_session_id: Some("conversation-1".to_string()),
        source_user_message_id: Some("message-1".to_string()),
        contact_agent_id: Some("contact-1".to_string()),
        default_model_config_id: Some("model-1".to_string()),
        expected_project_task_ids: vec!["task-1".to_string()],
        sandbox_target: Some(SandboxExecutionTarget {
            provider: SandboxProviderKind::Cloud,
            pairing_id: None,
            sandbox_id: "sandbox-1".to_string(),
            lease_id: "lease-1".to_string(),
            is_environment: false,
            service_id: None,
        }),
        project_context: ProjectExecutionContext {
            project_id: "project-1".to_string(),
            owner_user_id: "owner-1".to_string(),
            execution_plane: ExecutionPlane::Cloud,
            workspace_provider: WorkspaceProviderKind::CloudSandbox,
            workspace: None,
            sandbox_provider: SandboxProviderKind::Cloud,
            sandbox_pairing_id: None,
            source_type: Some("cloud".to_string()),
            revision: "project-revision-1".to_string(),
        },
        policy_revision: "policy-1".to_string(),
        route_revision: "route-1".to_string(),
        routes: Vec::new(),
        tools: Vec::new(),
        plugin_mcp_bindings: HashMap::from([(
            plugin_runtime.resource_id.clone(),
            plugin_runtime.clone(),
        )]),
        plugin_local_bindings: HashMap::from([(
            plugin_runtime.resource_id.clone(),
            PluginLocalProviderBinding {
                runtime: plugin_runtime,
                run_id: session_id.to_string(),
                device_id: "device-private-1".to_string(),
                workspace_id: "workspace-private-1".to_string(),
                adapter_session_id: "adapter-private-1".to_string(),
                operation: "mcp_tools_call".to_string(),
                session_sha256: "d".repeat(64),
                tool_snapshot_sha256: "e".repeat(64),
                tools: vec![serde_json::json!({
                    "name": "read_file",
                    "inputSchema": {"type": "object"}
                })],
                oauth_connection_id: Some("oauth-private-reference".to_string()),
                expires_at_unix,
            },
        )]),
        plugin_tool_component_bindings: Default::default(),
        plugin_local_tool_component_bindings: Default::default(),
        plugin_cloud_tool_component_bindings: Default::default(),
        external_http_bindings: HashMap::from([(
            "external-1".to_string(),
            ExternalHttpProviderBinding {
                provider_ref: "mcp-resource:external-1".to_string(),
                endpoint: reqwest::Url::parse("https://mcp.example.com/rpc").unwrap(),
                headers,
                http: reqwest::Client::new(),
                resolved_addresses: vec!["8.8.8.8:443".parse().unwrap()],
                allow_writes: false,
                allowed_tool_names: HashSet::from(["search".to_string()]),
                blocked_tool_names: HashSet::from(["delete".to_string()]),
            },
        )]),
        cloud_stdio_bindings: HashMap::from([(
            "stdio-1".to_string(),
            CloudStdioProviderBinding {
                provider_ref: "sandbox:sandbox-1/lease:lease-1".to_string(),
                command: "node".to_string(),
                args: vec!["server.js".to_string()],
                env: BTreeMap::from([(
                    "PLUGIN_TOKEN".to_string(),
                    "stdio-shared-store-secret".to_string(),
                )]),
                cwd: Some("/workspace/plugin".to_string()),
                plugin_artifact: None,
                allow_writes: true,
                allowed_tool_names: HashSet::new(),
                blocked_tool_names: HashSet::new(),
            },
        )]),
        expires_at: chrono::DateTime::from_timestamp(expires_at_unix, 0)
            .unwrap()
            .to_rfc3339(),
        expires_at_unix,
    }
}

#[tokio::test]
async fn memory_store_preserves_insert_get_and_atomic_remove_semantics() {
    let store = RuntimeSessionStore::memory();
    store.insert(snapshot("memory-session")).await.unwrap();
    assert_eq!(
        store
            .get("memory-session")
            .await
            .unwrap()
            .unwrap()
            .owner_user_id,
        "owner-1"
    );
    assert!(store.remove("memory-session").await.unwrap().is_some());
    assert!(store.get("memory-session").await.unwrap().is_none());
}

#[test]
fn encrypted_snapshot_roundtrip_preserves_private_bindings_without_plaintext_at_rest() {
    let cipher = SnapshotCipher::new("shared-session-encryption-secret").unwrap();
    let snapshot = snapshot("encrypted-session");
    let document = cipher.encrypt(&snapshot).unwrap();
    let encoded = mongodb::bson::to_vec(&document).unwrap();
    for secret in [
        b"shared-store-secret".as_slice(),
        b"stdio-shared-store-secret".as_slice(),
        b"/workspace/plugin".as_slice(),
        b"oauth-private-reference".as_slice(),
        b"plugin-private.example.com".as_slice(),
    ] {
        assert!(!encoded.windows(secret.len()).any(|window| window == secret));
    }

    let restored = cipher.decrypt(document, Duration::from_secs(60)).unwrap();
    let external = restored.external_http_bindings.get("external-1").unwrap();
    assert_eq!(
        external
            .headers
            .get("authorization")
            .unwrap()
            .to_str()
            .unwrap(),
        "Bearer shared-store-secret"
    );
    assert_eq!(external.resolved_addresses[0].to_string(), "8.8.8.8:443");
    assert_eq!(
        restored.cloud_stdio_bindings["stdio-1"].env["PLUGIN_TOKEN"],
        "stdio-shared-store-secret"
    );
    assert_eq!(
        restored.plugin_mcp_bindings["plugin-mcp-1"].release_id,
        "private-release-1"
    );
    assert_eq!(
        restored.plugin_local_bindings["plugin-mcp-1"].adapter_session_id,
        "adapter-private-1"
    );
}

#[test]
fn encrypted_snapshot_rejects_envelope_identity_tampering_and_wrong_keys() {
    let cipher = SnapshotCipher::new("shared-session-encryption-secret").unwrap();
    let mut document = cipher.encrypt(&snapshot("bound-session")).unwrap();
    document.session_id = "attacker-session".to_string();
    assert!(cipher
        .decrypt(document, Duration::from_secs(60))
        .unwrap_err()
        .contains("key mismatch or corrupted data"));

    let document = cipher.encrypt(&snapshot("wrong-key-session")).unwrap();
    let wrong_cipher = SnapshotCipher::new("another-encryption-secret").unwrap();
    assert!(wrong_cipher
        .decrypt(document, Duration::from_secs(60))
        .is_err());
}

#[test]
fn restored_external_http_binding_revalidates_pinned_public_addresses() {
    let binding = PersistedExternalHttpProviderBinding {
        provider_ref: "mcp-resource:external-1".to_string(),
        endpoint: "https://mcp.example.com/rpc".to_string(),
        headers: Vec::new(),
        resolved_addresses: vec!["127.0.0.1:443".to_string()],
        allow_writes: false,
        allowed_tool_names: HashSet::from(["search".to_string()]),
        blocked_tool_names: HashSet::new(),
    };
    assert!(restore_external_http_binding(binding, Duration::from_secs(60)).is_err());
}

#[tokio::test]
#[ignore = "requires CHATOS_MCP_MANAGEMENT_TEST_DATABASE_URL"]
async fn mongodb_store_is_shared_across_service_instances() {
    let database_url = std::env::var("CHATOS_MCP_MANAGEMENT_TEST_DATABASE_URL")
        .expect("CHATOS_MCP_MANAGEMENT_TEST_DATABASE_URL");
    let session_id = format!("shared-store-test-{}", uuid::Uuid::new_v4());
    let first = RuntimeSessionStore::connect(
        database_url.as_str(),
        "shared-session-encryption-secret",
        Duration::from_secs(60),
    )
    .await
    .unwrap();
    let second = RuntimeSessionStore::connect(
        database_url.as_str(),
        "shared-session-encryption-secret",
        Duration::from_secs(60),
    )
    .await
    .unwrap();

    first.insert(snapshot(session_id.as_str())).await.unwrap();
    assert_eq!(
        second
            .get(session_id.as_str())
            .await
            .unwrap()
            .unwrap()
            .route_revision,
        "route-1"
    );
    assert!(second.remove(session_id.as_str()).await.unwrap().is_some());
    assert!(first.get(session_id.as_str()).await.unwrap().is_none());
}
