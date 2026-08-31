// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_mcp_management_sdk::{
    ProjectExecutionContext, RuntimeWorkspaceRouteTarget, WorkspaceExecutionTarget,
    WorkspaceProviderKind,
};
use chatos_plugin_management_sdk::PluginMcpServer;

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
        installation_device_id: Some("device-private-1".to_string()),
        permission_snapshot: vec!["workspace.read".to_string()],
        auth_connection_ids: vec!["oauth-private-reference".to_string()],
        runtime: PluginMcpServer::Http {
            component_key: "workspace".to_string(),
            url: "https://plugin-private.example.com/mcp".to_string(),
            headers: Default::default(),
            oauth_resource: None,
            connect_timeout_ms: None,
            requires_exclusive_execution: false,
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
    RuntimeSessionSnapshot {
        session_id: session_id.to_string(),
        caller_service: "task-runner".to_string(),
        trace_id: "00000000-0000-4000-8000-000000000001".to_string(),
        tenant_id: "tenant-1".to_string(),
        owner_user_id: "owner-1".to_string(),
        owner_role: None,
        agent_key: chatos_plugin_management_sdk::SystemAgentKey::TaskRunnerRunPhase
            .as_str()
            .to_string(),
        task_profile: Some("default".to_string()),
        project_id: "project-1".to_string(),
        device_id: None,
        run_id: Some("run-1".to_string()),
        execution_group_id: Some("group-1".to_string()),
        execution_scope_generation: Some(1),
        turn_id: Some("turn-1".to_string()),
        task_id: Some("task-1".to_string()),
        task_title: Some("Task one".to_string()),
        source_session_id: Some("conversation-1".to_string()),
        source_user_message_id: Some("message-1".to_string()),
        contact_agent_id: Some("contact-1".to_string()),
        default_model_config_id: Some("model-1".to_string()),
        tool_result_max_chars: Some(40_000),
        expected_project_task_ids: vec!["task-1".to_string()],
        workspace_route: Some(RuntimeWorkspaceRouteTarget::LocalConnector {
            default_tool_root: Some("workspace".to_string()),
            owned_paths: Vec::new(),
        }),
        project_context: ProjectExecutionContext {
            project_id: "project-1".to_string(),
            owner_user_id: "owner-1".to_string(),
            workspace_provider: WorkspaceProviderKind::LocalConnector,
            workspace: Some(WorkspaceExecutionTarget {
                device_id: Some("device-private-1".to_string()),
                workspace_id: "workspace-private-1".to_string(),
                relative_root: None,
            }),
            revision: "project-revision-1".to_string(),
        },
        policy_revision: "policy-1".to_string(),
        route_revision: "route-1".to_string(),
        routes: Vec::new(),
        tools: Vec::new(),
        effective_mcp_ids: vec!["plugin-mcp-1".to_string()],
        provider_skills_prompt: Some("# Tool Usage Instructions".to_string()),
        plugin_instruction_items: vec![serde_json::json!({
            "role": "system",
            "content": "plugin instructions"
        })],
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
                workspace_id: Some("workspace-private-1".to_string()),
                adapter_session_id: "adapter-private-1".to_string(),
                operation: "mcp_tools_call".to_string(),
                session_sha256: "d".repeat(64),
                snapshot_sha256: "f".repeat(64),
                tool_snapshot_sha256: "e".repeat(64),
                server_instructions_sha256: "a".repeat(64),
                server_instructions: Some("Observe again after every UI mutation.".to_string()),
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
        local_connector_mcp_bindings: HashMap::from([(
            "external-1".to_string(),
            LocalConnectorMcpProviderBinding {
                provider_ref: "mcp-resource:external-1".to_string(),
                device_id: "device-private-1".to_string(),
                workspace_id: None,
                inline_http: Some(LocalConnectorInlineHttpRuntime {
                    url: "https://mcp.example.com/rpc".to_string(),
                    headers: std::collections::BTreeMap::from([(
                        "authorization".to_string(),
                        "Bearer shared-store-secret".to_string(),
                    )]),
                    timeout_ms: 30_000,
                }),
                allow_writes: false,
                allowed_tool_names: HashSet::from(["search".to_string()]),
                blocked_tool_names: HashSet::from(["delete".to_string()]),
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
    let first = store.get("memory-session").await.unwrap().unwrap();
    let second = store.get("memory-session").await.unwrap().unwrap();
    assert_eq!(first.tenant_id, "tenant-1");
    assert_eq!(first.owner_user_id, "owner-1");
    let routes = first.routes_response();
    assert_eq!(routes.effective_mcp_ids, ["plugin-mcp-1"]);
    assert_eq!(
        routes.provider_skills_prompt.as_deref(),
        Some("# Tool Usage Instructions")
    );
    assert_eq!(routes.plugin_instruction_items.len(), 1);
    assert!(Arc::ptr_eq(&first, &second));
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
        b"oauth-private-reference".as_slice(),
        b"plugin-private.example.com".as_slice(),
    ] {
        assert!(!encoded.windows(secret.len()).any(|window| window == secret));
    }

    let restored = cipher.decrypt(document).unwrap();
    assert_eq!(restored.trace_id, "00000000-0000-4000-8000-000000000001");
    assert_eq!(restored.tool_result_max_chars, Some(40_000));
    assert_eq!(restored.effective_mcp_ids, ["plugin-mcp-1"]);
    assert_eq!(
        restored.provider_skills_prompt.as_deref(),
        Some("# Tool Usage Instructions")
    );
    assert_eq!(restored.plugin_instruction_items.len(), 1);
    let external = restored
        .local_connector_mcp_bindings
        .get("external-1")
        .unwrap();
    assert_eq!(
        external
            .inline_http
            .as_ref()
            .unwrap()
            .headers
            .get("authorization")
            .unwrap()
            .as_str(),
        "Bearer shared-store-secret"
    );
    assert_eq!(external.device_id, "device-private-1");
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
        .decrypt(document)
        .unwrap_err()
        .contains("key mismatch or corrupted data"));

    let document = cipher.encrypt(&snapshot("wrong-key-session")).unwrap();
    let wrong_cipher = SnapshotCipher::new("another-encryption-secret").unwrap();
    assert!(wrong_cipher.decrypt(document).is_err());

    let mut old_schema = cipher.encrypt(&snapshot("old-schema-session")).unwrap();
    old_schema.schema_version = 4;
    assert!(cipher
        .decrypt(old_schema)
        .unwrap_err()
        .contains("unsupported Runtime Session Snapshot schema version"));
}

#[test]
fn cache_snapshot_evicts_oldest_entries_instead_of_clearing_everything() {
    let mut cache = RuntimeSessionCache::default();
    let first = snapshot("cache-first");
    let second = snapshot("cache-second");
    let third = snapshot("cache-third");

    cache_snapshot_with_limits(&mut cache, [1; 32], Arc::new(first), 2, usize::MAX);
    cache_snapshot_with_limits(&mut cache, [2; 32], Arc::new(second), 2, usize::MAX);
    cache_snapshot_with_limits(&mut cache, [3; 32], Arc::new(third), 2, usize::MAX);

    assert_eq!(cache.entries.len(), 2);
    assert!(!cache.entries.contains_key("cache-first"));
    assert!(cache.entries.contains_key("cache-second"));
    assert!(cache.entries.contains_key("cache-third"));
    assert_eq!(cache.capacity_evictions_total, 1);
}

#[test]
fn cache_snapshot_skips_entries_that_exceed_byte_budget() {
    let mut cache = RuntimeSessionCache::default();
    let oversized = snapshot("cache-oversized");
    let approx_size = estimate_snapshot_cache_bytes(&oversized);

    cache_snapshot_with_limits(
        &mut cache,
        [9; 32],
        Arc::new(oversized),
        16,
        approx_size.saturating_sub(1),
    );

    assert!(cache.entries.is_empty());
    assert_eq!(cache.total_bytes, 0);
    assert_eq!(cache.oversized_rejections_total, 1);
}

#[test]
fn cache_hits_share_the_same_snapshot_allocation() {
    let mut cache = RuntimeSessionCache::default();
    let snapshot = Arc::new(snapshot("cache-shared-arc"));
    let expires_at_unix = snapshot.expires_at_unix;

    cache_snapshot_with_limits(&mut cache, [7; 32], Arc::clone(&snapshot), 16, usize::MAX);
    let first = cache
        .get_if_fresh("cache-shared-arc", [7; 32], expires_at_unix - 1)
        .expect("first cache hit");
    let second = cache
        .get_if_fresh("cache-shared-arc", [7; 32], expires_at_unix - 1)
        .expect("second cache hit");

    assert!(Arc::ptr_eq(&snapshot, &first));
    assert!(Arc::ptr_eq(&first, &second));
    assert_eq!(cache.hits_total, 2);
    assert_eq!(cache.misses_total, 0);
}

#[test]
fn cache_counts_misses_digest_invalidations_and_expired_evictions() {
    let mut cache = RuntimeSessionCache::default();
    let mut expired = snapshot("cache-expired");
    expired.expires_at_unix = 10;
    cache_snapshot_with_limits(&mut cache, [1; 32], Arc::new(expired), 16, usize::MAX);

    assert!(cache.get_if_fresh("cache-missing", [1; 32], 9).is_none());
    assert!(cache.get_if_fresh("cache-expired", [1; 32], 10).is_none());

    let fresh = Arc::new(snapshot("cache-digest-mismatch"));
    cache_snapshot_with_limits(&mut cache, [2; 32], fresh, 16, usize::MAX);
    assert!(cache
        .get_if_fresh(
            "cache-digest-mismatch",
            [3; 32],
            chrono::Utc::now().timestamp(),
        )
        .is_none());

    assert_eq!(cache.hits_total, 0);
    assert_eq!(cache.misses_total, 3);
    assert_eq!(cache.expired_evictions_total, 1);
}

#[tokio::test]
async fn memory_store_stats_report_active_sessions_and_snapshot_sizes() {
    let store = RuntimeSessionStore::memory();
    store.insert(snapshot("stats-session-1")).await.unwrap();
    store.insert(snapshot("stats-session-2")).await.unwrap();

    let stats = store.stats().await.unwrap();

    assert_eq!(stats.backend, "memory");
    assert_eq!(stats.active_session_count, 2);
    assert_eq!(stats.cached_session_count, 2);
    assert!(stats.cached_total_bytes > 0);
    assert!(stats.cached_avg_snapshot_bytes > 0);
    assert!(stats.cached_p95_snapshot_bytes >= stats.cached_avg_snapshot_bytes);
    assert_eq!(stats.cache_entry_limit, None);
    assert_eq!(stats.cache_byte_limit, None);
    assert_eq!(stats.cache_hits_total, 0);
    assert_eq!(stats.cache_misses_total, 0);
    assert_eq!(stats.cache_capacity_evictions_total, 0);
    assert_eq!(stats.cache_expired_evictions_total, 0);
    assert_eq!(stats.cache_oversized_rejections_total, 0);
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
        RuntimeSessionCacheLimits::new(2_048, 32 * 1024 * 1024).unwrap(),
    )
    .await
    .unwrap();
    let second = RuntimeSessionStore::connect(
        database_url.as_str(),
        "shared-session-encryption-secret",
        RuntimeSessionCacheLimits::new(2_048, 32 * 1024 * 1024).unwrap(),
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
