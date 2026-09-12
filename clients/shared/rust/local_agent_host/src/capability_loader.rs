// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use chatos_agent_profiles::TaskRunnerExecutionTool;
use chatos_client_storage::{
    ClientStorage, ListQuery, PluginStateRecord, ProjectRecord, RecordPage, RecordQuery,
    RecordScope, StorageError, StorageResult, StorageTransaction, TransactionRepositories,
};
use chatos_mcp_client::{LocalMcpExecutor, LocalMcpServerConfig, StdioMcpExecutor};
use chatos_plugin_capability::{
    verify_signed_plugin_manifest, PluginReleaseSignature, PluginReleaseVerificationContext,
    SignedPluginManifest, TrustedPluginSigningKey,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::{
    capability_validation::{
        capability_revision, current_platform, local_server_name, require_identity,
        run_plugin_snapshot, selected_stdio_server, validate_environment_name,
        validate_environment_value, verify_component_status, verify_permissions,
        verify_resolved_executable_name, verify_runtime_declaration, MAXIMUM_RUNTIME_ENVIRONMENT,
    },
    RegisteredLocalCapabilityBundle, RegisteredLocalCapabilityRuntime,
    ValidatedLocalCapabilityReplacement,
};

pub const STORED_LOCAL_CAPABILITY_SCHEMA_VERSION: u32 = 2;

/// Final project-scoped representation written by native Plugin installation.
/// It contains immutable release and authorization evidence but never an
/// executable path or a runtime secret value.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StoredLocalCapabilityRecord {
    pub schema_version: u32,
    pub owner_user_id: String,
    pub device_id: String,
    pub project_id: String,
    pub policy_revision: String,
    pub marketplace_id: String,
    pub marketplace_source_kind: String,
    pub plugin_id: String,
    pub publisher_id: String,
    pub publisher_verified: bool,
    pub release: StoredSignedPluginRelease,
    pub authorization: StoredLocalPluginAuthorization,
    pub mcp_components: Vec<StoredLocalMcpComponent>,
    pub auth_connection_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StoredSignedPluginRelease {
    pub release_id: String,
    pub version: String,
    pub artifact_sha256: String,
    pub manifest_payload_base64: String,
    pub signature: PluginReleaseSignature,
    pub signing_key: TrustedPluginSigningKey,
    #[serde(default)]
    pub supported_platforms: Vec<String>,
    #[serde(default)]
    pub revoked_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StoredLocalPluginAuthorization {
    pub platform: String,
    pub active: bool,
    #[serde(default)]
    pub granted_permissions: Vec<String>,
    #[serde(default)]
    pub ready_component_keys: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StoredLocalMcpComponent {
    pub component_key: String,
    pub executable_reference: String,
    pub executable_sha256: String,
    #[serde(default)]
    pub arguments: Vec<String>,
    #[serde(default)]
    pub environment: BTreeMap<String, StoredLocalEnvironmentCredential>,
    pub tools: Vec<TaskRunnerExecutionTool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StoredLocalEnvironmentCredential {
    pub credential_name: String,
    pub reference: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedLocalMcpServer {
    pub name: String,
    pub executable: PathBuf,
    pub arguments: Vec<String>,
    pub environment: BTreeMap<String, String>,
    pub owner_user_id: String,
}

#[async_trait]
pub trait LocalCapabilityPlatform: Send + Sync {
    async fn resolve_plugin_executable(
        &self,
        reference: &str,
        expected_sha256: &str,
    ) -> Result<PathBuf, String>;

    async fn resolve_plugin_environment_secret(&self, reference: &str) -> Result<String, String>;
}

#[async_trait]
pub trait LocalCapabilityExecutorFactory: Send + Sync {
    async fn build(
        &self,
        servers: Vec<ResolvedLocalMcpServer>,
        allowed_tool_names: BTreeSet<String>,
    ) -> Result<Arc<dyn LocalMcpExecutor>, String>;
}

#[derive(Default)]
pub struct StdioLocalCapabilityExecutorFactory;

#[async_trait]
impl LocalCapabilityExecutorFactory for StdioLocalCapabilityExecutorFactory {
    async fn build(
        &self,
        servers: Vec<ResolvedLocalMcpServer>,
        allowed_tool_names: BTreeSet<String>,
    ) -> Result<Arc<dyn LocalMcpExecutor>, String> {
        let servers = servers
            .into_iter()
            .map(|server| {
                let working_directory = server
                    .executable
                    .parent()
                    .ok_or_else(|| "local Plugin executable has no parent directory".to_string())?
                    .to_path_buf();
                Ok(LocalMcpServerConfig {
                    name: server.name,
                    executable: server.executable,
                    arguments: server.arguments,
                    working_directory,
                    environment: server.environment,
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        StdioMcpExecutor::connect(servers, allowed_tool_names)
            .await
            .map(|executor| Arc::new(executor) as Arc<dyn LocalMcpExecutor>)
    }
}

pub struct StoredLocalCapabilityLoader {
    storage: Arc<dyn ClientStorage>,
    scope: RecordScope,
    device_id: String,
    platform: Arc<dyn LocalCapabilityPlatform>,
    executor_factory: Arc<dyn LocalCapabilityExecutorFactory>,
}

impl StoredLocalCapabilityLoader {
    pub fn new(
        storage: Arc<dyn ClientStorage>,
        scope: RecordScope,
        device_id: impl Into<String>,
        platform: Arc<dyn LocalCapabilityPlatform>,
    ) -> Result<Self, String> {
        Self::with_executor_factory(
            storage,
            scope,
            device_id,
            platform,
            Arc::new(StdioLocalCapabilityExecutorFactory),
        )
    }

    pub fn with_executor_factory(
        storage: Arc<dyn ClientStorage>,
        scope: RecordScope,
        device_id: impl Into<String>,
        platform: Arc<dyn LocalCapabilityPlatform>,
        executor_factory: Arc<dyn LocalCapabilityExecutorFactory>,
    ) -> Result<Self, String> {
        let device_id = device_id.into();
        require_identity("owner_user_id", scope.owner_user_id.as_str())?;
        require_identity("device_id", device_id.as_str())?;
        Ok(Self {
            storage,
            scope,
            device_id,
            platform,
            executor_factory,
        })
    }

    pub async fn load(&self, registry: &RegisteredLocalCapabilityRuntime) -> Result<usize, String> {
        let records = self.load_records().await?;
        let replacement = self.build_replacement(records).await?;
        let registered = replacement.len();
        registry.replace_validated(replacement);
        Ok(registered)
    }

    pub(crate) async fn build_replacement(
        &self,
        records: Vec<PluginStateRecord>,
    ) -> Result<ValidatedLocalCapabilityReplacement, String> {
        let mut projects = BTreeMap::<String, ProjectCapabilities>::new();
        for record in records {
            let verified = self.verify_record(record).await?;
            projects
                .entry(verified.project_id.clone())
                .or_insert_with(|| ProjectCapabilities::new(&verified))
                .add(verified)?;
        }
        let mut bundles = Vec::with_capacity(projects.len());
        for (_, project) in projects {
            bundles.push(project.build(self.executor_factory.as_ref()).await?);
        }
        RegisteredLocalCapabilityRuntime::validate_replacement(bundles)
    }

    pub(crate) async fn load_records(&self) -> Result<Vec<PluginStateRecord>, String> {
        let mut records = Vec::new();
        let mut cursor = None;
        loop {
            let mut operation = ListPluginRecords {
                query: ListQuery {
                    scope: self.scope.clone(),
                    cursor: cursor.clone(),
                    limit: ListQuery::MAX_LIMIT,
                },
                page: None,
            };
            self.storage
                .transaction(&mut operation)
                .await
                .map_err(|error| {
                    format!("failed to load installed Plugin capabilities: {error}")
                })?;
            let page = operation
                .page
                .ok_or_else(|| "Plugin capability query returned no page".to_string())?;
            records.extend(page.records);
            match page.next_cursor {
                Some(next) if Some(next.as_str()) != cursor.as_deref() => cursor = Some(next),
                Some(_) => {
                    return Err("Plugin capability pagination cursor did not advance".to_string())
                }
                None => break,
            }
        }
        Ok(records)
    }

    async fn verify_record(
        &self,
        record: PluginStateRecord,
    ) -> Result<VerifiedLocalCapability, String> {
        if record.metadata.scope != self.scope || record.metadata.origin_device_id != self.device_id
        {
            return Err(format!(
                "Plugin state record {} is outside the active owner or device",
                record.metadata.id
            ));
        }
        let stored: StoredLocalCapabilityRecord = serde_json::from_value(record.state.clone())
            .map_err(|error| {
                format!(
                    "Plugin state record {} does not match the final local capability schema: {error}",
                    record.metadata.id
                )
            })?;
        self.verify_identity(&record, &stored)?;
        self.verify_project(&stored).await?;
        let manifest = self.verify_release(&stored)?;
        let mut servers = Vec::with_capacity(stored.mcp_components.len());
        let mut tools = Vec::new();
        for component in &stored.mcp_components {
            let manifest_server = selected_stdio_server(&manifest, component)?;
            verify_runtime_declaration(component, manifest_server)?;
            let executable = self
                .platform
                .resolve_plugin_executable(
                    component.executable_reference.as_str(),
                    component.executable_sha256.as_str(),
                )
                .await?;
            verify_resolved_executable_name(executable.as_path(), manifest_server)?;
            let environment = self.resolve_environment(&component.environment).await?;
            servers.push(ResolvedLocalMcpServer {
                name: local_server_name(
                    record.plugin_id.as_str(),
                    component.component_key.as_str(),
                ),
                executable,
                arguments: component.arguments.clone(),
                environment,
                owner_user_id: self.scope.owner_user_id.clone(),
            });
            tools.extend(component.tools.iter().cloned());
        }
        let plugin_snapshot = run_plugin_snapshot(&stored);
        Ok(VerifiedLocalCapability {
            owner_user_id: self.scope.owner_user_id.clone(),
            project_id: stored.project_id,
            policy_revision: stored.policy_revision,
            plugin_snapshot,
            servers,
            tools,
        })
    }

    fn verify_identity(
        &self,
        record: &PluginStateRecord,
        stored: &StoredLocalCapabilityRecord,
    ) -> Result<(), String> {
        for (field, value) in [
            ("project_id", stored.project_id.as_str()),
            ("policy_revision", stored.policy_revision.as_str()),
        ] {
            require_identity(field, value)?;
        }
        if stored.mcp_components.is_empty() {
            return Err("local Plugin capability selects no MCP components".to_string());
        }
        let mut component_keys = BTreeSet::new();
        for component in &stored.mcp_components {
            require_identity("component_key", component.component_key.as_str())?;
            require_identity(
                "executable_reference",
                component.executable_reference.as_str(),
            )?;
            if !component_keys.insert(component.component_key.as_str()) {
                return Err("local Plugin capability contains duplicate MCP components".to_string());
            }
        }
        let mut auth_connection_ids = BTreeSet::new();
        for connection_id in &stored.auth_connection_ids {
            require_identity("auth_connection_id", connection_id)?;
            if !auth_connection_ids.insert(connection_id.as_str()) {
                return Err(
                    "local Plugin capability contains duplicate auth connections".to_string(),
                );
            }
        }
        if stored.schema_version != STORED_LOCAL_CAPABILITY_SCHEMA_VERSION
            || stored.owner_user_id != self.scope.owner_user_id
            || stored.device_id != self.device_id
            || record.plugin_id != stored.plugin_id
            || record.release != stored.release.release_id
            || stored.release.signature.marketplace_id != stored.marketplace_id
            || stored.release.signature.publisher_id != stored.publisher_id
        {
            return Err(format!(
                "Plugin state record {} has inconsistent owner, device, Release, or artifact identity",
                record.metadata.id
            ));
        }
        Ok(())
    }

    fn verify_release(
        &self,
        stored: &StoredLocalCapabilityRecord,
    ) -> Result<SignedPluginManifest, String> {
        let release = &stored.release;
        let authorization = &stored.authorization;
        if !matches!(
            stored.marketplace_source_kind.as_str(),
            "admin_registry" | "official_registry"
        ) || !stored.publisher_verified
            || release.revoked_at.is_some()
            || !authorization.active
        {
            return Err(
                "Plugin Release is not trusted, active, publisher-verified, and non-revoked"
                    .to_string(),
            );
        }
        if authorization.platform != current_platform()
            || (!release.supported_platforms.is_empty()
                && !release
                    .supported_platforms
                    .iter()
                    .any(|platform| platform == current_platform()))
        {
            return Err("Plugin Release is not valid for this Host platform".to_string());
        }
        let manifest = verify_signed_plugin_manifest(
            PluginReleaseVerificationContext {
                plugin_id: stored.plugin_id.as_str(),
                version: release.version.as_str(),
                marketplace_id: stored.marketplace_id.as_str(),
                publisher_id: stored.publisher_id.as_str(),
                artifact_sha256: release.artifact_sha256.as_str(),
            },
            release.manifest_payload_base64.as_str(),
            &release.signature,
            &release.signing_key,
        )
        .map_err(|error| format!("installed Plugin Release signature is invalid: {error}"))?;
        if !manifest.dependencies.supported_platforms.is_empty()
            && !manifest
                .dependencies
                .supported_platforms
                .iter()
                .any(|platform| platform == current_platform())
        {
            return Err("signed Plugin manifest does not support this Host platform".to_string());
        }
        for component in &stored.mcp_components {
            verify_permissions(stored, &manifest, component)?;
            verify_component_status(stored, component)?;
        }
        Ok(manifest)
    }

    async fn verify_project(&self, stored: &StoredLocalCapabilityRecord) -> Result<(), String> {
        let mut operation = LoadCapabilityProject {
            query: RecordQuery {
                scope: self.scope.clone(),
                id: stored.project_id.clone(),
            },
            project: None,
        };
        self.storage
            .transaction(&mut operation)
            .await
            .map_err(|error| format!("failed to verify Plugin capability project: {error}"))?;
        let project = operation
            .project
            .ok_or_else(|| "Plugin capability project is not registered locally".to_string())?;
        if project.metadata.scope != self.scope
            || project.metadata.id != stored.project_id
            || project.state.get("policy_revision").and_then(Value::as_str)
                != Some(stored.policy_revision.as_str())
        {
            return Err(
                "Plugin capability project or policy revision does not match local state"
                    .to_string(),
            );
        }
        Ok(())
    }

    async fn resolve_environment(
        &self,
        environment: &BTreeMap<String, StoredLocalEnvironmentCredential>,
    ) -> Result<BTreeMap<String, String>, String> {
        if environment.len() > MAXIMUM_RUNTIME_ENVIRONMENT {
            return Err("local Plugin environment contains too many entries".to_string());
        }
        let mut resolved = BTreeMap::new();
        for (name, value) in environment {
            validate_environment_name(name)?;
            require_identity(
                "environment credential name",
                value.credential_name.as_str(),
            )?;
            require_identity("environment secret reference", value.reference.as_str())?;
            let value = self
                .platform
                .resolve_plugin_environment_secret(value.reference.as_str())
                .await?;
            validate_environment_value(value.as_str())?;
            resolved.insert(name.clone(), value);
        }
        Ok(resolved)
    }
}

struct ListPluginRecords {
    query: ListQuery,
    page: Option<RecordPage<PluginStateRecord>>,
}

struct LoadCapabilityProject {
    query: RecordQuery,
    project: Option<ProjectRecord>,
}

#[async_trait]
impl StorageTransaction for LoadCapabilityProject {
    async fn execute(
        &mut self,
        repositories: &mut dyn TransactionRepositories,
    ) -> StorageResult<()> {
        self.project = Some(
            repositories
                .projects()
                .get(&self.query)
                .await?
                .ok_or(StorageError::NotFound)?,
        );
        Ok(())
    }
}

#[async_trait]
impl StorageTransaction for ListPluginRecords {
    async fn execute(
        &mut self,
        repositories: &mut dyn TransactionRepositories,
    ) -> StorageResult<()> {
        self.page = Some(repositories.plugins().list(&self.query).await?);
        Ok(())
    }
}

struct VerifiedLocalCapability {
    owner_user_id: String,
    project_id: String,
    policy_revision: String,
    plugin_snapshot: Value,
    servers: Vec<ResolvedLocalMcpServer>,
    tools: Vec<TaskRunnerExecutionTool>,
}

struct ProjectCapabilities {
    owner_user_id: String,
    project_id: String,
    policy_revision: String,
    plugins: Vec<Value>,
    servers: Vec<ResolvedLocalMcpServer>,
    tools: Vec<TaskRunnerExecutionTool>,
    plugin_ids: BTreeSet<String>,
    tool_names: BTreeSet<String>,
}

impl ProjectCapabilities {
    fn new(capability: &VerifiedLocalCapability) -> Self {
        Self {
            owner_user_id: capability.owner_user_id.clone(),
            project_id: capability.project_id.clone(),
            policy_revision: capability.policy_revision.clone(),
            plugins: Vec::new(),
            servers: Vec::new(),
            tools: Vec::new(),
            plugin_ids: BTreeSet::new(),
            tool_names: BTreeSet::new(),
        }
    }

    fn add(&mut self, capability: VerifiedLocalCapability) -> Result<(), String> {
        if capability.project_id != self.project_id
            || capability.policy_revision != self.policy_revision
            || capability.owner_user_id != self.owner_user_id
            || capability
                .servers
                .iter()
                .any(|server| server.owner_user_id != self.owner_user_id)
        {
            return Err(
                "project Plugin capabilities do not share one frozen identity and policy revision"
                    .to_string(),
            );
        }
        if !self.plugin_ids.insert(
            capability
                .plugin_snapshot
                .get("plugin_id")
                .and_then(Value::as_str)
                .ok_or_else(|| "Plugin snapshot has no plugin_id".to_string())?
                .to_string(),
        ) {
            return Err(format!(
                "project {} contains duplicate Plugin {} capability records",
                self.project_id,
                capability
                    .plugin_snapshot
                    .get("plugin_id")
                    .and_then(Value::as_str)
                    .unwrap_or("unknown")
            ));
        }
        for tool in &capability.tools {
            if !self.tool_names.insert(tool.name.clone()) {
                return Err(format!(
                    "project {} contains duplicate MCP tool {}",
                    self.project_id, tool.name
                ));
            }
        }
        self.plugins.push(capability.plugin_snapshot);
        self.servers.extend(capability.servers);
        self.tools.extend(capability.tools);
        Ok(())
    }

    async fn build(
        mut self,
        executor_factory: &dyn LocalCapabilityExecutorFactory,
    ) -> Result<RegisteredLocalCapabilityBundle, String> {
        self.plugins.sort_by(|left, right| {
            left.get("plugin_id")
                .and_then(Value::as_str)
                .cmp(&right.get("plugin_id").and_then(Value::as_str))
        });
        self.tools.sort_by(|left, right| left.name.cmp(&right.name));
        self.servers
            .sort_by(|left, right| left.name.cmp(&right.name));
        let project_id = self.project_id;
        let plugin_release_snapshot = json!({
            "schema_version": STORED_LOCAL_CAPABILITY_SCHEMA_VERSION,
            "project_id": project_id,
            "policy_revision": self.policy_revision,
            "plugins": self.plugins,
        });
        let resolution_revision = capability_revision(&plugin_release_snapshot, &self.tools)?;
        let executor = executor_factory
            .build(self.servers, self.tool_names)
            .await?;
        Ok(RegisteredLocalCapabilityBundle {
            owner_user_id: self.owner_user_id,
            project_id,
            resolution_revision,
            plugin_release_snapshot,
            execution_tools: self.tools,
            executor,
        })
    }
}
