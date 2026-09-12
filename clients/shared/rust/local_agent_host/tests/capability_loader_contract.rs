// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use base64::{engine::general_purpose::STANDARD, Engine as _};
use chatos_agent_profiles::{TaskRunnerExecutionTool, TaskRunnerProjectSnapshot};
use chatos_client_storage::{
    ClientStorage, ListQuery, PluginStateRecord, ProjectRecord, PutRecord, RecordMetadata,
    RecordPage, RecordScope, SecretReference, SqliteBootstrapProfile, SqliteClientStorage,
    StorageEncryptionKey, StorageResult, StorageTransaction, TransactionRepositories,
};
use chatos_local_agent_host::{
    LocalAgentIpcMutationExecutor, LocalCapabilityExecutorFactory, LocalCapabilityIpcExecutor,
    LocalCapabilityPlatform, LocalTaskCapabilityRequest, LocalTaskCapabilityResolver,
    RegisteredLocalCapabilityRuntime, ResolvedLocalMcpServer, StoredLocalCapabilityLoader,
    StoredLocalCapabilityRecord, StoredLocalMcpComponent, StoredLocalPluginAuthorization,
    StoredSignedPluginRelease, STORED_LOCAL_CAPABILITY_SCHEMA_VERSION,
};
use chatos_local_agent_protocol::{
    InstallProjectPluginCapabilityCommand, LocalAgentCommand, LocalAgentIpcError,
    LocalAgentIpcResponse, RemoveProjectPluginCapabilityCommand, ToolEffect,
};
use chatos_mcp_client::{LocalMcpExecutor, LocalMcpToolCall, LocalMcpToolResult};
use chatos_plugin_capability::{
    plugin_release_signing_payload, PluginReleaseSignature, PluginReleaseVerificationContext,
    TrustedPluginSigningKey, PLUGIN_SIGNATURE_ALGORITHM_ED25519, PLUGIN_SIGNING_KEY_USAGE_RELEASE,
};
use chrono::Utc;
use ring::rand::SystemRandom;
use ring::signature::{Ed25519KeyPair, KeyPair};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use tokio_util::sync::CancellationToken;

const ARTIFACT_SHA256: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const EXECUTABLE_SHA256: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
const PUBLIC_TOOL_NAME: &str = "plugin-plugin-demo-demo-mcp_read_demo";
type CapabilityMutation = Box<dyn Fn(&mut StoredLocalCapabilityRecord)>;

struct Platform {
    resolutions: Mutex<Vec<(String, String)>>,
}

impl Platform {
    fn new() -> Self {
        Self {
            resolutions: Mutex::new(Vec::new()),
        }
    }
}

#[async_trait]
impl LocalCapabilityPlatform for Platform {
    async fn resolve_plugin_executable(
        &self,
        reference: &str,
        expected_sha256: &str,
    ) -> Result<PathBuf, String> {
        self.resolutions
            .lock()
            .unwrap()
            .push((reference.to_string(), expected_sha256.to_string()));
        Ok(PathBuf::from("/private/plugins/demo-plugin-bin"))
    }

    async fn resolve_plugin_environment_secret(&self, _reference: &str) -> Result<String, String> {
        Ok("private-plugin-token".to_string())
    }
}

struct FixtureExecutor {
    tools: Vec<Value>,
}

#[async_trait]
impl LocalMcpExecutor for FixtureExecutor {
    fn available_tools(&self) -> Vec<Value> {
        self.tools.clone()
    }

    async fn execute_tool(
        &self,
        _call: LocalMcpToolCall,
        _cancellation: CancellationToken,
    ) -> Result<LocalMcpToolResult, String> {
        Ok(LocalMcpToolResult {
            content: "ok".to_string(),
            structured_result: None,
            is_error: false,
            fatal_error: false,
        })
    }
}

struct Factory {
    servers: Mutex<Vec<ResolvedLocalMcpServer>>,
    add_extra_tool: bool,
    drift_schema: bool,
}

impl Factory {
    fn exact() -> Self {
        Self {
            servers: Mutex::new(Vec::new()),
            add_extra_tool: true,
            drift_schema: false,
        }
    }

    fn drifting() -> Self {
        Self {
            servers: Mutex::new(Vec::new()),
            add_extra_tool: false,
            drift_schema: true,
        }
    }
}

#[async_trait]
impl LocalCapabilityExecutorFactory for Factory {
    async fn build(
        &self,
        servers: Vec<ResolvedLocalMcpServer>,
        allowed_tool_names: BTreeSet<String>,
    ) -> Result<Arc<dyn LocalMcpExecutor>, String> {
        servers
            .first()
            .map(|server| server.name.as_str())
            .ok_or_else(|| "fixture requires one server".to_string())?;
        *self.servers.lock().unwrap() = servers;
        let mut tools = vec![json!({
            "type": "function",
            "name": PUBLIC_TOOL_NAME,
            "description": "Read demo data",
            "parameters": if self.drift_schema {
                json!({"type": "object", "properties": {"changed": {"type": "boolean"}}})
            } else {
                json!({
                    "type": "object",
                    "properties": {"query": {"type": "string"}},
                    "additionalProperties": false
                })
            },
        })];
        if self.add_extra_tool {
            tools.push(json!({
                "type": "function",
                "name": "plugin-plugin-demo-demo-mcp_undeclared_write",
                "description": "Must be hidden",
                "parameters": {"type": "object", "properties": {}},
            }));
        }
        tools.retain(|tool| {
            tool.get("name")
                .and_then(Value::as_str)
                .is_some_and(|name| allowed_tool_names.contains(name))
        });
        Ok(Arc::new(FixtureExecutor { tools }))
    }
}

struct PutFixtures {
    project: Option<ProjectRecord>,
    plugin: Option<PluginStateRecord>,
}

#[async_trait]
impl StorageTransaction for PutFixtures {
    async fn execute(
        &mut self,
        repositories: &mut dyn TransactionRepositories,
    ) -> StorageResult<()> {
        repositories
            .projects()
            .put(PutRecord {
                record: self.project.take().unwrap(),
                expected_revision: None,
            })
            .await?;
        if let Some(plugin) = self.plugin.take() {
            repositories
                .plugins()
                .put(PutRecord {
                    record: plugin,
                    expected_revision: None,
                })
                .await?;
        }
        Ok(())
    }
}

struct RejectTail;

#[async_trait]
impl LocalAgentIpcMutationExecutor for RejectTail {
    async fn execute_mutation(
        &self,
        _request_id: &str,
        _command: LocalAgentCommand,
    ) -> Result<LocalAgentIpcResponse, LocalAgentIpcError> {
        Err(LocalAgentIpcError {
            code: "unexpected_test_command".to_string(),
            message: "unexpected test command".to_string(),
            retryable: false,
        })
    }
}

struct ListStoredPlugins {
    page: Option<RecordPage<PluginStateRecord>>,
}

#[async_trait]
impl StorageTransaction for ListStoredPlugins {
    async fn execute(
        &mut self,
        repositories: &mut dyn TransactionRepositories,
    ) -> StorageResult<()> {
        self.page = Some(
            repositories
                .plugins()
                .list(&ListQuery {
                    scope: scope(),
                    cursor: None,
                    limit: ListQuery::MAX_LIMIT,
                })
                .await?,
        );
        Ok(())
    }
}

#[tokio::test]
async fn loads_verified_project_capabilities_and_hides_undeclared_tools() {
    let directory = tempfile::tempdir().unwrap();
    let storage = storage(directory.path()).await;
    put_record(storage.as_ref(), plugin_record(signed_capability())).await;
    let platform = Arc::new(Platform::new());
    let factory = Arc::new(Factory::exact());
    let loader = StoredLocalCapabilityLoader::with_executor_factory(
        storage,
        scope(),
        "device-1",
        platform.clone(),
        factory.clone(),
    )
    .unwrap();
    let registry = RegisteredLocalCapabilityRuntime::new();

    assert_eq!(loader.load(&registry).await.unwrap(), 1);
    let resolution = registry
        .resolve_capabilities(&capability_request(), CancellationToken::new())
        .await
        .unwrap();
    assert_eq!(resolution.execution_tools.len(), 1);
    assert_eq!(resolution.execution_tools[0].name, PUBLIC_TOOL_NAME);
    assert_eq!(
        resolution.plugin_release_snapshot["project_id"],
        "project-1"
    );
    assert_eq!(
        resolution.plugin_release_snapshot["plugins"][0]["release_id"],
        "release-1"
    );
    assert_eq!(
        platform.resolutions.lock().unwrap().as_slice(),
        &[(
            "executable-grant-1".to_string(),
            EXECUTABLE_SHA256.to_string()
        )]
    );
    let servers = factory.servers.lock().unwrap();
    assert_eq!(servers.len(), 1);
    assert_eq!(servers[0].arguments, ["serve"]);
    assert_eq!(
        servers[0].environment.get("API_TOKEN").map(String::as_str),
        Some("private-plugin-token")
    );
    assert!(!resolution
        .plugin_release_snapshot
        .to_string()
        .contains("private-plugin-token"));
}

#[tokio::test]
async fn rejects_signature_permission_identity_and_release_drift_before_execution() {
    let mutations: Vec<CapabilityMutation> = vec![
        Box::new(|record| record.release.signature.signature_base64 = STANDARD.encode([0_u8; 64])),
        Box::new(|record| record.authorization.granted_permissions.clear()),
        Box::new(|record| record.device_id = "other-device".to_string()),
        Box::new(|record| record.project_id = "project-2".to_string()),
        Box::new(|record| record.release.artifact_sha256 = "c".repeat(64)),
        Box::new(|record| {
            record.mcp_components[0]
                .environment
                .get_mut("API_TOKEN")
                .unwrap()
                .credential_name = "other-token".to_string()
        }),
        Box::new(|record| record.release.revoked_at = Some("2026-09-12T00:00:00Z".to_string())),
        Box::new(|record| record.authorization.active = false),
    ];
    for mutate in mutations {
        let directory = tempfile::tempdir().unwrap();
        let storage = storage(directory.path()).await;
        let mut capability = signed_capability();
        mutate(&mut capability);
        put_record(storage.as_ref(), plugin_record(capability)).await;
        let platform = Arc::new(Platform::new());
        let loader = StoredLocalCapabilityLoader::with_executor_factory(
            storage,
            scope(),
            "device-1",
            platform.clone(),
            Arc::new(Factory::exact()),
        )
        .unwrap();

        assert!(loader
            .load(&RegisteredLocalCapabilityRuntime::new())
            .await
            .is_err());
        assert!(platform.resolutions.lock().unwrap().is_empty());
    }
}

#[tokio::test]
async fn rejects_tools_list_schema_drift_after_local_mcp_initialization() {
    let directory = tempfile::tempdir().unwrap();
    let storage = storage(directory.path()).await;
    put_record(storage.as_ref(), plugin_record(signed_capability())).await;
    let loader = StoredLocalCapabilityLoader::with_executor_factory(
        storage,
        scope(),
        "device-1",
        Arc::new(Platform::new()),
        Arc::new(Factory::drifting()),
    )
    .unwrap();

    let error = loader
        .load(&RegisteredLocalCapabilityRuntime::new())
        .await
        .unwrap_err();
    assert!(error.contains("schema differs"), "{error}");
}

#[tokio::test]
async fn ipc_installs_idempotently_then_removes_one_verified_project_capability() {
    let directory = tempfile::tempdir().unwrap();
    let storage = storage(directory.path()).await;
    put_project_only(storage.as_ref()).await;
    let registry = Arc::new(RegisteredLocalCapabilityRuntime::new());
    let loader = Arc::new(
        StoredLocalCapabilityLoader::with_executor_factory(
            storage.clone(),
            scope(),
            "device-1",
            Arc::new(Platform::new()),
            Arc::new(Factory::exact()),
        )
        .unwrap(),
    );
    loader.load(registry.as_ref()).await.unwrap();
    let executor = LocalCapabilityIpcExecutor::new(
        storage.clone(),
        scope(),
        "device-1",
        loader,
        registry.clone(),
        Arc::new(RejectTail),
    );
    let capability = serde_json::to_value(signed_capability()).unwrap();
    let install = || {
        LocalAgentCommand::InstallProjectPluginCapability(InstallProjectPluginCapabilityCommand {
            project_id: "project-1".to_string(),
            plugin_id: "plugin-demo".to_string(),
            release_id: "release-1".to_string(),
            capability_record: capability.clone(),
        })
    };

    assert_eq!(
        executor
            .execute_mutation("install-1", install())
            .await
            .unwrap(),
        LocalAgentIpcResponse::Success
    );
    let first = stored_plugins(storage.as_ref()).await;
    assert_eq!(first.len(), 1);
    assert_eq!(first[0].metadata.revision, 1);
    assert!(registry
        .resolve_capabilities(&capability_request(), CancellationToken::new())
        .await
        .is_ok());

    assert_eq!(
        executor
            .execute_mutation("install-1", install())
            .await
            .unwrap(),
        LocalAgentIpcResponse::Success
    );
    let retried = stored_plugins(storage.as_ref()).await;
    assert_eq!(retried.len(), 1);
    assert_eq!(retried[0].metadata.revision, 1);

    let mismatch = executor
        .execute_mutation(
            "remove-stale",
            LocalAgentCommand::RemoveProjectPluginCapability(
                RemoveProjectPluginCapabilityCommand {
                    project_id: "project-1".to_string(),
                    plugin_id: "plugin-demo".to_string(),
                    release_id: "release-stale".to_string(),
                },
            ),
        )
        .await
        .unwrap_err();
    assert_eq!(mismatch.code, "plugin_release_mismatch");
    assert_eq!(stored_plugins(storage.as_ref()).await.len(), 1);

    assert_eq!(
        executor
            .execute_mutation(
                "remove-1",
                LocalAgentCommand::RemoveProjectPluginCapability(
                    RemoveProjectPluginCapabilityCommand {
                        project_id: "project-1".to_string(),
                        plugin_id: "plugin-demo".to_string(),
                        release_id: "release-1".to_string(),
                    },
                ),
            )
            .await
            .unwrap(),
        LocalAgentIpcResponse::Success
    );
    assert!(stored_plugins(storage.as_ref()).await.is_empty());
    assert!(registry
        .resolve_capabilities(&capability_request(), CancellationToken::new())
        .await
        .is_err());
}

#[tokio::test]
async fn ipc_rejects_invalid_capability_before_storage_or_registry_changes() {
    let directory = tempfile::tempdir().unwrap();
    let storage = storage(directory.path()).await;
    put_project_only(storage.as_ref()).await;
    let registry = Arc::new(RegisteredLocalCapabilityRuntime::new());
    let loader = Arc::new(
        StoredLocalCapabilityLoader::with_executor_factory(
            storage.clone(),
            scope(),
            "device-1",
            Arc::new(Platform::new()),
            Arc::new(Factory::exact()),
        )
        .unwrap(),
    );
    let executor = LocalCapabilityIpcExecutor::new(
        storage.clone(),
        scope(),
        "device-1",
        loader,
        registry.clone(),
        Arc::new(RejectTail),
    );
    let mut capability = signed_capability();
    capability.release.signature.signature_base64 = STANDARD.encode([0_u8; 64]);

    let error = executor
        .execute_mutation(
            "install-invalid",
            LocalAgentCommand::InstallProjectPluginCapability(
                InstallProjectPluginCapabilityCommand {
                    project_id: "project-1".to_string(),
                    plugin_id: "plugin-demo".to_string(),
                    release_id: "release-1".to_string(),
                    capability_record: serde_json::to_value(capability).unwrap(),
                },
            ),
        )
        .await
        .unwrap_err();
    assert_eq!(error.code, "plugin_capability_rejected");
    assert!(stored_plugins(storage.as_ref()).await.is_empty());
    assert!(registry.is_empty().unwrap());
}

async fn storage(root: &std::path::Path) -> Arc<dyn ClientStorage> {
    Arc::new(
        SqliteClientStorage::open(
            &SqliteBootstrapProfile {
                database_path: root.join("client.sqlite"),
                encryption_secret: SecretReference::new("test-key").unwrap(),
            },
            &StorageEncryptionKey::new([7; 32]),
        )
        .await
        .unwrap(),
    )
}

async fn put_project_only(storage: &dyn ClientStorage) {
    storage
        .transaction(&mut PutFixtures {
            project: Some(project_record()),
            plugin: None,
        })
        .await
        .unwrap();
}

async fn stored_plugins(storage: &dyn ClientStorage) -> Vec<PluginStateRecord> {
    let mut operation = ListStoredPlugins { page: None };
    storage.transaction(&mut operation).await.unwrap();
    operation.page.unwrap().records
}

async fn put_record(storage: &dyn ClientStorage, record: PluginStateRecord) {
    storage
        .transaction(&mut PutFixtures {
            project: Some(project_record()),
            plugin: Some(record),
        })
        .await
        .unwrap();
}

fn project_record() -> ProjectRecord {
    ProjectRecord {
        metadata: RecordMetadata {
            id: "project-1".to_string(),
            scope: scope(),
            origin_device_id: "device-1".to_string(),
            revision: 1,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        },
        name: "Demo project".to_string(),
        root_reference: Some("workspace-grant-1".to_string()),
        state: json!({"policy_revision": "policy-1"}),
    }
}

fn plugin_record(capability: StoredLocalCapabilityRecord) -> PluginStateRecord {
    PluginStateRecord {
        metadata: RecordMetadata {
            id: "plugin-state-1".to_string(),
            scope: scope(),
            origin_device_id: "device-1".to_string(),
            revision: 1,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        },
        plugin_id: "plugin-demo".to_string(),
        release: "release-1".to_string(),
        state: serde_json::to_value(capability).unwrap(),
    }
}

fn signed_capability() -> StoredLocalCapabilityRecord {
    let manifest_bytes = serde_json::to_vec(&json!({
        "schemaVersion": 3,
        "name": "demo-plugin",
        "version": "1.0.0",
        "mcpServers": [{
            "transport": "stdio",
            "component_key": "demo-mcp",
            "bin": "demo-plugin-bin",
            "args": ["serve"],
            "env": {"API_TOKEN": "${credential:api-token}"}
        }],
        "dependencies": {"supportedPlatforms": ["macos", "windows", "linux"]},
        "permissions": [{
            "permission": "process.spawn",
            "required": true,
            "components": ["demo-mcp"]
        }]
    }))
    .unwrap();
    let keypair_bytes = Ed25519KeyPair::generate_pkcs8(&SystemRandom::new()).unwrap();
    let keypair = Ed25519KeyPair::from_pkcs8(keypair_bytes.as_ref()).unwrap();
    let mut signature = PluginReleaseSignature {
        key_id: "key-1".to_string(),
        publisher_id: "publisher-1".to_string(),
        marketplace_id: "marketplace-1".to_string(),
        algorithm: PLUGIN_SIGNATURE_ALGORITHM_ED25519.to_string(),
        signature_base64: String::new(),
        signed_at: "2026-09-12T00:00:00Z".to_string(),
        manifest_sha256: format!("{:x}", Sha256::digest(&manifest_bytes)),
    };
    let signature_context = PluginReleaseVerificationContext {
        plugin_id: "plugin-demo",
        version: "1.0.0",
        marketplace_id: "marketplace-1",
        publisher_id: "publisher-1",
        artifact_sha256: ARTIFACT_SHA256,
    };
    let payload = plugin_release_signing_payload(signature_context, &signature).unwrap();
    signature.signature_base64 = STANDARD.encode(keypair.sign(payload.as_slice()).as_ref());
    StoredLocalCapabilityRecord {
        schema_version: STORED_LOCAL_CAPABILITY_SCHEMA_VERSION,
        owner_user_id: "user-1".to_string(),
        device_id: "device-1".to_string(),
        project_id: "project-1".to_string(),
        policy_revision: "policy-1".to_string(),
        marketplace_id: "marketplace-1".to_string(),
        marketplace_source_kind: "official_registry".to_string(),
        plugin_id: "plugin-demo".to_string(),
        publisher_id: "publisher-1".to_string(),
        publisher_verified: true,
        release: StoredSignedPluginRelease {
            release_id: "release-1".to_string(),
            version: "1.0.0".to_string(),
            artifact_sha256: ARTIFACT_SHA256.to_string(),
            manifest_payload_base64: STANDARD.encode(&manifest_bytes),
            signature,
            signing_key: TrustedPluginSigningKey {
                key_id: "key-1".to_string(),
                publisher_id: "publisher-1".to_string(),
                algorithm: PLUGIN_SIGNATURE_ALGORITHM_ED25519.to_string(),
                public_key_base64: STANDARD.encode(keypair.public_key().as_ref()),
                usages: vec![PLUGIN_SIGNING_KEY_USAGE_RELEASE.to_string()],
                valid_from: "2026-09-11T00:00:00Z".to_string(),
                valid_until: Some("2027-09-12T00:00:00Z".to_string()),
                revoked_at: None,
            },
            supported_platforms: vec![current_platform().to_string()],
            revoked_at: None,
        },
        authorization: StoredLocalPluginAuthorization {
            platform: current_platform().to_string(),
            active: true,
            granted_permissions: vec!["process.spawn".to_string()],
            ready_component_keys: vec!["demo-mcp".to_string()],
        },
        mcp_components: vec![StoredLocalMcpComponent {
            component_key: "demo-mcp".to_string(),
            executable_reference: "executable-grant-1".to_string(),
            executable_sha256: EXECUTABLE_SHA256.to_string(),
            arguments: vec!["serve".to_string()],
            environment: BTreeMap::from([(
                "API_TOKEN".to_string(),
                chatos_local_agent_host::StoredLocalEnvironmentCredential {
                    credential_name: "api-token".to_string(),
                    reference: "plugin-secret-1".to_string(),
                },
            )]),
            tools: vec![TaskRunnerExecutionTool {
                name: PUBLIC_TOOL_NAME.to_string(),
                effect: ToolEffect::Read,
                schema: json!({
                    "type": "function",
                    "name": PUBLIC_TOOL_NAME,
                    "description": "Read demo data",
                    "parameters": {
                        "type": "object",
                        "properties": {"query": {"type": "string"}},
                        "additionalProperties": false
                    }
                }),
            }],
        }],
        auth_connection_ids: Vec::new(),
    }
}

fn capability_request() -> LocalTaskCapabilityRequest {
    LocalTaskCapabilityRequest {
        task_id: "task-1".to_string(),
        owner_user_id: "user-1".to_string(),
        project_snapshot: TaskRunnerProjectSnapshot {
            project_id: "project-1".to_string(),
            snapshot_revision: "project-revision-1".to_string(),
            working_directory_ref: "workspace-grant-1".to_string(),
            authority_snapshot: json!({}),
        },
        objective: "Read data".to_string(),
        acceptance_criteria: vec!["Data was read".to_string()],
        parent_capability_snapshot_ref: "parent-capability-1".to_string(),
    }
}

fn scope() -> RecordScope {
    RecordScope {
        owner_user_id: "user-1".to_string(),
    }
}

#[cfg(target_os = "macos")]
const fn current_platform() -> &'static str {
    "macos"
}

#[cfg(windows)]
const fn current_platform() -> &'static str {
    "windows"
}

#[cfg(target_os = "linux")]
const fn current_platform() -> &'static str {
    "linux"
}
