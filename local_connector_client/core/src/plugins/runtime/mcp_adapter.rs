// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::{anyhow, bail, Context, Result};
use async_trait::async_trait;
use chatos_mcp_runtime::{
    extract_tools, invalidate_stdio_session, jsonrpc_http_call, jsonrpc_stdio_call,
    parse_tool_definition, McpStdioServer,
};
use chatos_plugin_management_sdk::{
    normalize_plugin_relative_path, normalized_plugin_manifest_sha256, parse_plugin_manifest,
    plugin_manifest_source_from_path, PluginComponentKind, PluginMcpServer,
};
use chrono::{SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use tokio_util::sync::CancellationToken;
use zeroize::Zeroize;

use super::mcp_config::load_configured_mcp_server;
use super::mcp_credentials::{
    PluginCredentialBindings, PluginHttpHeaderTemplates, PluginStdioEnvironmentTemplates,
};
use super::oauth_broker::{PluginOAuthBroker, PluginOAuthTokenBinding};
use super::stdio_sandbox::{PluginStdioSandboxLauncher, PluginStdioSandboxRuntime};
use crate::plugins::{ActivePluginInstallation, PluginCredentialVault, PluginInstaller};

const MAX_MANIFEST_BYTES: u64 = 1024 * 1024;
const MAX_MCP_TOOLS: usize = 200;
const MAX_MCP_TOOL_SNAPSHOT_BYTES: usize = 512 * 1024;
const MCP_TOOL_CALL_OPERATION: &str = "mcp_tools_call";
const MCP_HEALTH_CHECK_OPERATION: &str = "mcp_health_check";
const MCP_HEALTH_PROBE_INTERVAL: Duration = Duration::from_secs(60);
const MCP_HEALTHY_STATUS: &str = "healthy";
const MCP_DEGRADED_STATUS: &str = "degraded";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PluginMcpSnapshot {
    pub plugin_id: String,
    pub release_id: String,
    pub version: String,
    pub artifact_sha256: String,
    pub component_key: String,
    pub server_key: String,
    pub transport: String,
    pub credential_snapshot_sha256: Option<String>,
    pub oauth_connection_id: Option<String>,
    pub oauth_snapshot_sha256: Option<String>,
    pub tools: Vec<Value>,
    pub tool_snapshot_sha256: String,
    pub snapshot_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginMcpHealthSnapshot {
    pub status: String,
    pub checked_at: String,
    pub last_success_at: Option<String>,
    pub consecutive_failures: u32,
}

#[derive(Debug)]
struct PluginMcpHealthState {
    snapshot: PluginMcpHealthSnapshot,
    checked_at: Instant,
}

impl PluginMcpHealthState {
    fn healthy() -> Self {
        let checked_at = health_timestamp();
        Self {
            snapshot: PluginMcpHealthSnapshot {
                status: MCP_HEALTHY_STATUS.to_string(),
                checked_at: checked_at.clone(),
                last_success_at: Some(checked_at),
                consecutive_failures: 0,
            },
            checked_at: Instant::now(),
        }
    }

    fn record_success(&mut self) -> PluginMcpHealthSnapshot {
        let checked_at = health_timestamp();
        self.snapshot = PluginMcpHealthSnapshot {
            status: MCP_HEALTHY_STATUS.to_string(),
            checked_at: checked_at.clone(),
            last_success_at: Some(checked_at),
            consecutive_failures: 0,
        };
        self.checked_at = Instant::now();
        self.snapshot.clone()
    }

    fn record_failure(&mut self) -> PluginMcpHealthSnapshot {
        self.snapshot.status = MCP_DEGRADED_STATUS.to_string();
        self.snapshot.checked_at = health_timestamp();
        self.snapshot.consecutive_failures = self.snapshot.consecutive_failures.saturating_add(1);
        self.checked_at = Instant::now();
        self.snapshot.clone()
    }
}

#[derive(Clone)]
pub(super) struct PreparedPluginMcp {
    snapshot: PluginMcpSnapshot,
    transport: PreparedPluginMcpTransport,
    published_tools: BTreeSet<String>,
    invoker: Arc<dyn PluginMcpInvoker>,
    installer: PluginInstaller,
    health: Arc<Mutex<PluginMcpHealthState>>,
    health_probe_lock: Arc<tokio::sync::Mutex<()>>,
}

impl std::fmt::Debug for PreparedPluginMcp {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PreparedPluginMcp")
            .field("snapshot", &self.snapshot)
            .field("transport", &self.transport)
            .field("published_tools", &self.published_tools)
            .field("health", &self.health_snapshot().ok())
            .finish_non_exhaustive()
    }
}

impl PreparedPluginMcp {
    pub(super) fn snapshot(&self) -> &PluginMcpSnapshot {
        &self.snapshot
    }

    pub(super) fn operation(&self) -> &'static str {
        MCP_TOOL_CALL_OPERATION
    }

    pub(super) fn health_operation(&self) -> &'static str {
        MCP_HEALTH_CHECK_OPERATION
    }

    pub(super) fn publishes_tool(&self, tool_name: &str) -> bool {
        self.published_tools.contains(tool_name)
    }

    pub(super) fn validate_active(&self) -> Result<()> {
        validate_active_mcp_snapshot(&self.installer, &self.snapshot)?;
        self.transport.verify_bindings()
    }

    pub(super) fn health_snapshot(&self) -> Result<PluginMcpHealthSnapshot> {
        self.health
            .lock()
            .map(|health| health.snapshot.clone())
            .map_err(|_| anyhow!("Plugin MCP health state is unavailable"))
    }

    #[cfg(test)]
    pub(super) fn expire_health_probe_for_tests(&self) {
        if let Ok(mut health) = self.health.lock() {
            health.checked_at = Instant::now() - MCP_HEALTH_PROBE_INTERVAL;
        }
    }

    pub(super) async fn check_health(&self) -> Result<PluginMcpHealthSnapshot> {
        let _probe_guard = self.health_probe_lock.lock().await;
        self.probe_health().await
    }

    pub(super) async fn call_tool(&self, tool_name: &str, arguments: Value) -> Result<Value> {
        self.ensure_recent_health().await?;
        self.invoker
            .call(
                &self.transport,
                "tools/call",
                json!({"name": tool_name, "arguments": arguments}),
            )
            .await
    }

    pub(super) fn cancel(&self) {
        self.invoker.cancel(&self.transport);
    }

    async fn ensure_recent_health(&self) -> Result<()> {
        if !self.health_probe_due()? {
            return Ok(());
        }
        let _probe_guard = self.health_probe_lock.lock().await;
        if !self.health_probe_due()? {
            return Ok(());
        }
        let health = self.probe_health().await?;
        if health.status != MCP_HEALTHY_STATUS {
            bail!("Plugin MCP health probe failed");
        }
        Ok(())
    }

    fn health_probe_due(&self) -> Result<bool> {
        self.health
            .lock()
            .map(|health| health.checked_at.elapsed() >= MCP_HEALTH_PROBE_INTERVAL)
            .map_err(|_| anyhow!("Plugin MCP health state is unavailable"))
    }

    async fn probe_health(&self) -> Result<PluginMcpHealthSnapshot> {
        let healthy = match self
            .invoker
            .call(&self.transport, "tools/list", json!({}))
            .await
        {
            Ok(response) => sanitize_tools(response, &self.published_tools, &BTreeSet::new())
                .and_then(|tools| sha256_json(&tools))
                .is_ok_and(|sha256| sha256 == self.snapshot.tool_snapshot_sha256),
            Err(_) => false,
        };
        let mut health = self
            .health
            .lock()
            .map_err(|_| anyhow!("Plugin MCP health state is unavailable"))?;
        Ok(if healthy {
            health.record_success()
        } else {
            health.record_failure()
        })
    }
}

#[derive(Debug, Clone)]
pub(super) enum PreparedPluginMcpTransport {
    Stdio {
        server: McpStdioServer,
        environment: PluginStdioEnvironmentTemplates,
        credential_bindings: Option<PluginCredentialBindings>,
        cancellation: CancellationToken,
        _sandbox_runtime: Option<Arc<PluginStdioSandboxRuntime>>,
    },
    Http {
        url: String,
        headers: PluginHttpHeaderTemplates,
        credential_bindings: Option<PluginCredentialBindings>,
        oauth_binding: Option<Box<PluginOAuthTokenBinding>>,
        cancellation: CancellationToken,
        timeout: Duration,
    },
}

impl PreparedPluginMcpTransport {
    fn credential_snapshot_sha256(&self) -> Option<&str> {
        match self {
            Self::Stdio {
                credential_bindings,
                ..
            } => credential_bindings
                .as_ref()
                .map(PluginCredentialBindings::snapshot_sha256),
            Self::Http {
                credential_bindings,
                ..
            } => credential_bindings
                .as_ref()
                .map(PluginCredentialBindings::snapshot_sha256),
        }
    }

    fn oauth_snapshot_sha256(&self) -> Option<&str> {
        match self {
            Self::Stdio { .. } => None,
            Self::Http { oauth_binding, .. } => oauth_binding
                .as_deref()
                .map(PluginOAuthTokenBinding::snapshot_sha256),
        }
    }

    fn oauth_connection_id(&self) -> Option<&str> {
        match self {
            Self::Stdio { .. } => None,
            Self::Http { oauth_binding, .. } => oauth_binding
                .as_deref()
                .map(PluginOAuthTokenBinding::connection_id),
        }
    }

    fn verify_bindings(&self) -> Result<()> {
        match self {
            Self::Stdio {
                credential_bindings,
                ..
            } => {
                if let Some(bindings) = credential_bindings {
                    bindings.verify()?;
                }
                Ok(())
            }
            Self::Http {
                credential_bindings,
                oauth_binding,
                ..
            } => {
                if let Some(bindings) = credential_bindings {
                    bindings.verify()?;
                }
                if let Some(binding) = oauth_binding {
                    binding.verify()?;
                }
                Ok(())
            }
        }
    }
}

#[async_trait]
pub(super) trait PluginMcpInvoker: Send + Sync {
    async fn call(
        &self,
        transport: &PreparedPluginMcpTransport,
        method: &str,
        params: Value,
    ) -> Result<Value>;

    fn cancel(&self, transport: &PreparedPluginMcpTransport);
}

#[derive(Debug)]
struct DefaultPluginMcpInvoker;

#[async_trait]
impl PluginMcpInvoker for DefaultPluginMcpInvoker {
    async fn call(
        &self,
        transport: &PreparedPluginMcpTransport,
        method: &str,
        params: Value,
    ) -> Result<Value> {
        match transport {
            PreparedPluginMcpTransport::Stdio {
                server,
                environment,
                credential_bindings,
                cancellation,
                ..
            } => {
                let request = async {
                    let values = environment.resolve(credential_bindings.as_ref())?;
                    let server = ResolvedPluginStdioServer::new(server, values.cloned_map());
                    jsonrpc_stdio_call(server.as_server(), method, params, None)
                        .await
                        .map_err(anyhow::Error::msg)
                };
                tokio::select! {
                    _ = cancellation.cancelled() => {
                        bail!("Plugin MCP stdio request was cancelled")
                    }
                    result = request => result,
                }
            }
            PreparedPluginMcpTransport::Http {
                url,
                headers,
                credential_bindings,
                oauth_binding,
                cancellation,
                timeout,
            } => {
                let request = async {
                    let mut headers = headers.resolve(credential_bindings.as_ref())?;
                    if let Some(binding) = oauth_binding {
                        let token = binding.resolve().await?;
                        headers.insert(
                            "authorization".to_string(),
                            format!("Bearer {}", token.as_str()),
                        );
                    }
                    jsonrpc_http_call(url, Some(headers.as_map()), method, params, Some(*timeout))
                        .await
                        .map_err(anyhow::Error::msg)
                };
                tokio::select! {
                    _ = cancellation.cancelled() => {
                        bail!("Plugin MCP HTTP request was cancelled")
                    }
                    result = request => result,
                }
            }
        }
    }

    fn cancel(&self, transport: &PreparedPluginMcpTransport) {
        match transport {
            PreparedPluginMcpTransport::Stdio {
                server,
                cancellation,
                ..
            } => {
                cancellation.cancel();
                invalidate_stdio_session(server);
            }
            PreparedPluginMcpTransport::Http { cancellation, .. } => cancellation.cancel(),
        }
    }
}

struct ResolvedPluginStdioServer(McpStdioServer);

impl ResolvedPluginStdioServer {
    fn new(
        server: &McpStdioServer,
        environment: std::collections::HashMap<String, String>,
    ) -> Self {
        let mut server = server.clone();
        if !environment.is_empty() {
            server.env = Some(environment);
        }
        Self(server)
    }

    fn as_server(&self) -> &McpStdioServer {
        &self.0
    }
}

impl Drop for ResolvedPluginStdioServer {
    fn drop(&mut self) {
        if let Some(environment) = &mut self.0.env {
            for value in environment.values_mut() {
                value.zeroize();
            }
        }
    }
}

#[derive(Clone)]
pub struct PluginMcpAdapter {
    installer: PluginInstaller,
    credential_vault: Option<PluginCredentialVault>,
    oauth_broker: Option<PluginOAuthBroker>,
    invoker: Arc<dyn PluginMcpInvoker>,
    stdio_execution_enabled: bool,
    stdio_sandbox_launcher: Option<PluginStdioSandboxLauncher>,
    stdio_unavailable_reason: String,
}

impl std::fmt::Debug for PluginMcpAdapter {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PluginMcpAdapter")
            .field("plugin_root", &self.installer.plugin_root())
            .finish_non_exhaustive()
    }
}

impl PluginMcpAdapter {
    pub fn new(installer: PluginInstaller) -> Self {
        let credential_vault = installer.credential_vault();
        let sandbox_requested = !cfg!(test)
            && std::env::var("LOCAL_CONNECTOR_ENABLE_PLUGIN_STDIO_SANDBOX").as_deref() == Ok("1");
        let sandbox = sandbox_requested
            .then(PluginStdioSandboxLauncher::discover)
            .transpose();
        let (stdio_sandbox_launcher, stdio_unavailable_reason) = match sandbox {
            Ok(Some(launcher)) => (Some(launcher), String::new()),
            Ok(None) => (
                None,
                "Plugin stdio MCP execution requires OS sandbox isolation and is disabled by Local Connector configuration"
                    .to_string(),
            ),
            Err(error) => (None, error.to_string()),
        };
        Self {
            installer,
            credential_vault,
            oauth_broker: None,
            invoker: Arc::new(DefaultPluginMcpInvoker),
            stdio_execution_enabled: stdio_sandbox_launcher.is_some(),
            stdio_sandbox_launcher,
            stdio_unavailable_reason,
        }
    }

    #[cfg(test)]
    pub(super) fn with_invoker(
        installer: PluginInstaller,
        invoker: Arc<dyn PluginMcpInvoker>,
    ) -> Self {
        let credential_vault = installer.credential_vault();
        Self {
            installer,
            credential_vault,
            oauth_broker: None,
            invoker,
            stdio_execution_enabled: true,
            stdio_sandbox_launcher: None,
            stdio_unavailable_reason: String::new(),
        }
    }

    #[cfg(test)]
    pub(super) fn with_stdio_execution_for_tests(installer: PluginInstaller) -> Self {
        let credential_vault = installer.credential_vault();
        Self {
            installer,
            credential_vault,
            oauth_broker: None,
            invoker: Arc::new(DefaultPluginMcpInvoker),
            stdio_execution_enabled: true,
            stdio_sandbox_launcher: None,
            stdio_unavailable_reason: String::new(),
        }
    }

    #[cfg(test)]
    pub(super) fn with_stdio_sandbox_for_tests(
        installer: PluginInstaller,
        launcher: PluginStdioSandboxLauncher,
    ) -> Self {
        let credential_vault = installer.credential_vault();
        Self {
            installer,
            credential_vault,
            oauth_broker: None,
            invoker: Arc::new(DefaultPluginMcpInvoker),
            stdio_execution_enabled: true,
            stdio_sandbox_launcher: Some(launcher),
            stdio_unavailable_reason: String::new(),
        }
    }

    pub fn with_oauth_broker(mut self, oauth_broker: PluginOAuthBroker) -> Self {
        self.oauth_broker = Some(oauth_broker);
        self
    }

    pub(super) async fn prepare(
        &self,
        plugin_id: &str,
        component_key: &str,
        requested_server_key: Option<&str>,
        adapter_session_id: &str,
        owner_user_id: &str,
        device_id: &str,
        permission_snapshot: &BTreeSet<String>,
        tool_allowlist: &BTreeSet<String>,
        tool_blocklist: &BTreeSet<String>,
    ) -> Result<PreparedPluginMcp> {
        let installation = self
            .installer
            .active_installation(plugin_id)?
            .context("Plugin is not installed and active")?;
        let component = installation
            .version
            .inventory
            .components
            .iter()
            .find(|component| component.component_key == component_key)
            .context("Plugin component is not present in the signed installation inventory")?;
        if component.kind != PluginComponentKind::McpServer {
            bail!("Plugin component is not an MCP server");
        }
        validate_required_permissions(&installation, component_key, permission_snapshot)?;
        let manifest = load_verified_manifest(&installation)?;
        let declared_server = manifest
            .mcp_servers
            .iter()
            .find(|server| server.component_key() == component_key)
            .context("Plugin MCP component is not present in the normalized Manifest")?;
        let server = match declared_server {
            PluginMcpServer::ConfigFile { path, .. } => {
                load_configured_mcp_server(&installation, path, requested_server_key)?
            }
            server => {
                if requested_server_key.is_some_and(|server_key| server_key != component_key) {
                    bail!("inline Plugin MCP component server_key must match component_key");
                }
                server.clone()
            }
        };
        let server_key = server.component_key().to_string();
        let transport = prepare_transport(
            &installation,
            &server,
            adapter_session_id,
            owner_user_id,
            device_id,
            component_key,
            permission_snapshot,
            self.stdio_execution_enabled,
            self.stdio_sandbox_launcher.as_ref(),
            self.stdio_unavailable_reason.as_str(),
            self.credential_vault.clone(),
            self.oauth_broker.clone(),
        )?;
        let response = self
            .invoker
            .call(&transport, "tools/list", json!({}))
            .await
            .context("discover Plugin MCP tools")?;
        let tools = sanitize_tools(response, tool_allowlist, tool_blocklist)?;
        let published_tools = tools
            .iter()
            .filter_map(parse_tool_definition)
            .map(|tool| tool.name)
            .collect::<BTreeSet<_>>();
        let tool_snapshot_sha256 = sha256_json(&tools)?;
        let transport_name = match &transport {
            PreparedPluginMcpTransport::Stdio { .. } => "stdio",
            PreparedPluginMcpTransport::Http { .. } => "http",
        }
        .to_string();
        let snapshot_sha256 = mcp_snapshot_sha256(
            &installation,
            component_key,
            server_key.as_str(),
            transport_name.as_str(),
            tool_snapshot_sha256.as_str(),
            transport.credential_snapshot_sha256(),
            transport.oauth_snapshot_sha256(),
        );
        let credential_snapshot_sha256 = transport.credential_snapshot_sha256().map(str::to_string);
        let oauth_connection_id = transport.oauth_connection_id().map(str::to_string);
        let oauth_snapshot_sha256 = transport.oauth_snapshot_sha256().map(str::to_string);
        Ok(PreparedPluginMcp {
            snapshot: PluginMcpSnapshot {
                plugin_id: installation.plugin_id.clone(),
                release_id: installation.version.release_id.clone(),
                version: installation.version.version.clone(),
                artifact_sha256: installation.version.artifact_sha256.clone(),
                component_key: component_key.to_string(),
                server_key,
                transport: transport_name,
                credential_snapshot_sha256,
                oauth_connection_id,
                oauth_snapshot_sha256,
                tools,
                tool_snapshot_sha256,
                snapshot_sha256,
            },
            transport,
            published_tools,
            invoker: self.invoker.clone(),
            installer: self.installer.clone(),
            health: Arc::new(Mutex::new(PluginMcpHealthState::healthy())),
            health_probe_lock: Arc::new(tokio::sync::Mutex::new(())),
        })
    }
}

pub(super) fn load_verified_manifest(
    installation: &ActivePluginInstallation,
) -> Result<chatos_plugin_management_sdk::PluginManifest> {
    let relative_path = [".chatos-plugin/plugin.json", ".codex-plugin/plugin.json"]
        .into_iter()
        .find(|path| installation.version.package_file_sha256.contains_key(*path))
        .context("installed Plugin has no checksummed Manifest")?;
    let path = installation.installation_path.join(relative_path);
    let metadata = fs::symlink_metadata(path.as_path())
        .with_context(|| format!("read installed Plugin Manifest metadata: {relative_path}"))?;
    if !metadata.is_file()
        || metadata.file_type().is_symlink()
        || metadata.len() > MAX_MANIFEST_BYTES
    {
        bail!("installed Plugin Manifest is unsafe or exceeds its size limit");
    }
    let raw = fs::read_to_string(path.as_path()).context("read installed Plugin Manifest")?;
    let source = plugin_manifest_source_from_path(Path::new(relative_path))
        .context("derive installed Plugin Manifest source")?;
    let manifest = parse_plugin_manifest(raw.as_str(), source)
        .context("parse installed normalized Plugin Manifest")?;
    if normalized_plugin_manifest_sha256(&manifest)? != installation.version.manifest_sha256 {
        bail!("installed Plugin Manifest does not match the active signed Release");
    }
    Ok(manifest)
}

fn validate_required_permissions(
    installation: &ActivePluginInstallation,
    component_key: &str,
    permission_snapshot: &BTreeSet<String>,
) -> Result<()> {
    for requirement in installation
        .version
        .inventory
        .permissions
        .iter()
        .filter(|requirement| {
            requirement.required
                && (requirement.components.is_empty()
                    || requirement
                        .components
                        .iter()
                        .any(|key| key == component_key))
        })
    {
        if !permission_snapshot.contains(requirement.permission.as_str()) {
            bail!(
                "Plugin MCP required permission is missing from the prepared snapshot: {}",
                requirement.permission
            );
        }
    }
    Ok(())
}

fn prepare_transport(
    installation: &ActivePluginInstallation,
    server: &PluginMcpServer,
    adapter_session_id: &str,
    owner_user_id: &str,
    device_id: &str,
    credential_component_key: &str,
    permission_snapshot: &BTreeSet<String>,
    stdio_execution_enabled: bool,
    stdio_sandbox_launcher: Option<&PluginStdioSandboxLauncher>,
    stdio_unavailable_reason: &str,
    credential_vault: Option<PluginCredentialVault>,
    oauth_broker: Option<PluginOAuthBroker>,
) -> Result<PreparedPluginMcpTransport> {
    match server {
        PluginMcpServer::ConfigFile { .. } => {
            bail!("Plugin MCP config-file components are not supported yet")
        }
        PluginMcpServer::Stdio {
            command,
            args,
            env,
            cwd,
            ..
        } => {
            if !stdio_execution_enabled {
                bail!(stdio_unavailable_reason.to_string());
            }
            if !permission_snapshot.contains("process.spawn") {
                bail!("Plugin stdio MCP requires process.spawn in the permission snapshot");
            }
            let environment = PluginStdioEnvironmentTemplates::parse(env)?;
            if !environment.secret_names().is_empty() {
                validate_credential_permission(
                    installation,
                    credential_component_key,
                    permission_snapshot,
                )?;
            }
            let credential_bindings = PluginCredentialBindings::prepare(
                credential_vault,
                owner_user_id,
                device_id,
                installation.plugin_id.as_str(),
                installation.version.release_id.as_str(),
                credential_component_key,
                environment.secret_names(),
            )?;
            validate_arguments(args)?;
            let command = resolve_signed_command(installation, command)?;
            let cwd = resolve_cwd(installation, cwd.as_ref().map(|path| path.path.as_str()))?;
            let server_name = format!(
                "plugin:{}:{}:{}",
                installation.plugin_id,
                installation.version.release_id,
                server.component_key()
            );
            let server = McpStdioServer::new(server_name, command.to_string_lossy().into_owned())
                .with_args(args.clone())
                .with_cwd(cwd.to_string_lossy().into_owned())
                .with_user_id(format!("{owner_user_id}:{device_id}:{adapter_session_id}"));
            let (server, sandbox_runtime) = match stdio_sandbox_launcher {
                Some(launcher) => {
                    let (server, runtime) = launcher.prepare(
                        installation
                            .installation_path
                            .parent()
                            .unwrap_or(installation.installation_path.as_path()),
                        installation.installation_path.as_path(),
                        &server,
                        environment.variable_names(),
                        &installation.version.package_file_sha256,
                    )?;
                    (server, Some(runtime))
                }
                None => (server, None),
            };
            Ok(PreparedPluginMcpTransport::Stdio {
                server,
                environment,
                credential_bindings,
                cancellation: CancellationToken::new(),
                _sandbox_runtime: sandbox_runtime,
            })
        }
        PluginMcpServer::Http {
            url,
            headers,
            oauth_resource,
            connect_timeout_ms,
            ..
        } => {
            validate_http_permission(url, permission_snapshot)?;
            let header_templates = PluginHttpHeaderTemplates::parse(headers)?;
            if !header_templates.secret_names().is_empty() {
                validate_credential_permission(
                    installation,
                    credential_component_key,
                    permission_snapshot,
                )?;
            }
            let credential_bindings = PluginCredentialBindings::prepare(
                credential_vault,
                owner_user_id,
                device_id,
                installation.plugin_id.as_str(),
                installation.version.release_id.as_str(),
                credential_component_key,
                header_templates.secret_names(),
            )?;
            let oauth_binding = match oauth_resource.as_deref() {
                Some(resource) => {
                    if header_templates.contains("authorization") {
                        bail!(
                            "Plugin HTTP MCP cannot combine oauth_resource with an Authorization header template"
                        );
                    }
                    let broker = oauth_broker
                        .context("Plugin OAuth HTTP MCP requires the local OAuth Broker")?;
                    let binding = broker.prepare_token_binding(
                        owner_user_id,
                        device_id,
                        installation.plugin_id.as_str(),
                        installation.version.release_id.as_str(),
                        resource,
                    )?;
                    validate_oauth_permissions(
                        installation,
                        credential_component_key,
                        binding.provider(),
                        binding.scopes(),
                        permission_snapshot,
                    )?;
                    Some(Box::new(binding))
                }
                None => None,
            };
            Ok(PreparedPluginMcpTransport::Http {
                url: url.clone(),
                headers: header_templates,
                credential_bindings,
                oauth_binding,
                cancellation: CancellationToken::new(),
                timeout: Duration::from_millis(
                    connect_timeout_ms.unwrap_or(30_000).clamp(300, 120_000),
                ),
            })
        }
    }
}

fn resolve_signed_command(
    installation: &ActivePluginInstallation,
    command: &str,
) -> Result<PathBuf> {
    if !command.contains('/') {
        bail!("reviewed Plugin MCP command identifiers are not enabled yet");
    }
    let relative = normalize_plugin_relative_path(command)
        .map_err(|message| anyhow!("invalid Plugin MCP command path: {message}"))?;
    let relative = relative.trim_start_matches("./");
    if !installation
        .version
        .package_file_sha256
        .contains_key(relative)
    {
        bail!("Plugin MCP command is not covered by package checksums");
    }
    let path = installation.installation_path.join(relative);
    let metadata = fs::symlink_metadata(path.as_path()).context("read Plugin MCP command")?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        bail!("Plugin MCP command is not a safe regular file");
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o111 == 0 {
            bail!("Plugin MCP command is not executable");
        }
    }
    Ok(path)
}

fn resolve_cwd(installation: &ActivePluginInstallation, cwd: Option<&str>) -> Result<PathBuf> {
    let Some(cwd) = cwd else {
        return Ok(installation.installation_path.clone());
    };
    let relative = normalize_plugin_relative_path(cwd)
        .map_err(|message| anyhow!("invalid Plugin MCP cwd: {message}"))?;
    let path = installation
        .installation_path
        .join(relative.trim_start_matches("./"));
    let metadata = fs::symlink_metadata(path.as_path()).context("read Plugin MCP cwd")?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        bail!("Plugin MCP cwd is not a safe directory");
    }
    Ok(path)
}

fn validate_arguments(args: &[String]) -> Result<()> {
    if args.len() > 128 {
        bail!("Plugin MCP command has too many arguments");
    }
    if args.iter().any(|arg| {
        arg.len() > 8 * 1024
            || arg.contains('\0')
            || matches!(arg.as_str(), "-c" | "--eval" | "--execute")
    }) {
        bail!("Plugin MCP command contains an unsafe or oversized argument");
    }
    Ok(())
}

fn validate_http_permission(url: &str, permission_snapshot: &BTreeSet<String>) -> Result<()> {
    let parsed = reqwest::Url::parse(url).context("parse Plugin MCP HTTP URL")?;
    let host = parsed
        .host_str()
        .context("Plugin MCP HTTP URL is missing a host")?;
    let loopback = host.eq_ignore_ascii_case("localhost")
        || host
            .parse::<std::net::IpAddr>()
            .ok()
            .is_some_and(|address| address.is_loopback());
    if parsed.scheme() != "https" && !(parsed.scheme() == "http" && loopback) {
        bail!("Plugin HTTP MCP requires HTTPS, except for loopback development servers");
    }
    let permission = format!("network.domain:{}", host.to_ascii_lowercase());
    if !permission_snapshot.contains(permission.as_str()) {
        bail!("Plugin HTTP MCP requires {permission} in the permission snapshot");
    }
    Ok(())
}

fn validate_credential_permission(
    installation: &ActivePluginInstallation,
    component_key: &str,
    permission_snapshot: &BTreeSet<String>,
) -> Result<()> {
    let declared = installation
        .version
        .inventory
        .permissions
        .iter()
        .filter(|requirement| {
            requirement.components.is_empty()
                || requirement
                    .components
                    .iter()
                    .any(|key| key == component_key)
        })
        .map(|requirement| requirement.permission.as_str())
        .filter(|permission| {
            *permission == "credential.use" || permission.starts_with("credential.use:")
        })
        .collect::<BTreeSet<_>>();
    if declared.is_empty()
        || !declared
            .iter()
            .any(|permission| permission_snapshot.contains(*permission))
    {
        bail!(
            "Plugin MCP credential template requires a signed credential.use permission in the prepared snapshot"
        );
    }
    Ok(())
}

fn validate_oauth_permissions(
    installation: &ActivePluginInstallation,
    component_key: &str,
    provider: &str,
    scopes: &[String],
    permission_snapshot: &BTreeSet<String>,
) -> Result<()> {
    let declared = installation
        .version
        .inventory
        .permissions
        .iter()
        .filter(|requirement| {
            requirement.components.is_empty()
                || requirement
                    .components
                    .iter()
                    .any(|key| key == component_key)
        })
        .map(|requirement| requirement.permission.as_str())
        .collect::<BTreeSet<_>>();
    for scope in scopes {
        let permission = format!("oauth.scope:{provider}:{scope}");
        if !declared.contains(permission.as_str())
            || !permission_snapshot.contains(permission.as_str())
        {
            bail!("Plugin OAuth MCP requires signed permission: {permission}");
        }
    }
    Ok(())
}

fn sanitize_tools(
    response: Value,
    tool_allowlist: &BTreeSet<String>,
    tool_blocklist: &BTreeSet<String>,
) -> Result<Vec<Value>> {
    let mut tools = extract_tools(&response)
        .map_err(anyhow::Error::msg)?
        .into_iter()
        .filter_map(|tool| parse_tool_definition(&tool).map(|parsed| (parsed.name, tool)))
        .filter(|(name, _)| {
            (tool_allowlist.is_empty() || tool_allowlist.contains(name))
                && !tool_blocklist.contains(name)
        })
        .collect::<Vec<_>>();
    tools.sort_by(|left, right| left.0.cmp(&right.0));
    tools.dedup_by(|left, right| left.0 == right.0);
    if tools.is_empty() {
        bail!("Plugin MCP tools/list returned no permitted valid tools");
    }
    if tools.len() > MAX_MCP_TOOLS {
        bail!("Plugin MCP tool catalog exceeds the tool count limit");
    }
    let tools = tools.into_iter().map(|(_, tool)| tool).collect::<Vec<_>>();
    if serde_json::to_vec(&tools)?.len() > MAX_MCP_TOOL_SNAPSHOT_BYTES {
        bail!("Plugin MCP tool snapshot exceeds the byte limit");
    }
    Ok(tools)
}

fn validate_active_mcp_snapshot(
    installer: &PluginInstaller,
    snapshot: &PluginMcpSnapshot,
) -> Result<()> {
    let installation = installer
        .active_installation(snapshot.plugin_id.as_str())?
        .context("Plugin is no longer installed and active")?;
    if installation.version.release_id != snapshot.release_id
        || installation.version.version != snapshot.version
        || installation.version.artifact_sha256 != snapshot.artifact_sha256
        || !installation
            .version
            .inventory
            .components
            .iter()
            .any(|component| {
                component.component_key == snapshot.component_key
                    && component.kind == PluginComponentKind::McpServer
            })
    {
        bail!("Plugin MCP snapshot does not match the active immutable Release");
    }
    Ok(())
}

fn mcp_snapshot_sha256(
    installation: &ActivePluginInstallation,
    component_key: &str,
    server_key: &str,
    transport: &str,
    tool_snapshot_sha256: &str,
    credential_snapshot_sha256: Option<&str>,
    oauth_snapshot_sha256: Option<&str>,
) -> String {
    let mut payload = format!(
        "chatos.plugin.mcp.snapshot.v1\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}",
        installation.plugin_id,
        installation.version.release_id,
        installation.version.version,
        installation.version.artifact_sha256,
        component_key,
        server_key,
        transport,
        tool_snapshot_sha256,
    );
    if let Some(credential_snapshot_sha256) = credential_snapshot_sha256 {
        payload.push('\n');
        payload.push_str(credential_snapshot_sha256);
    }
    if let Some(oauth_snapshot_sha256) = oauth_snapshot_sha256 {
        payload.push('\n');
        payload.push_str(oauth_snapshot_sha256);
    }
    hex::encode(Sha256::digest(payload.as_bytes()))
}

fn sha256_json<T: Serialize>(value: &T) -> Result<String> {
    Ok(hex::encode(Sha256::digest(serde_json::to_vec(value)?)))
}

fn health_timestamp() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true)
}
