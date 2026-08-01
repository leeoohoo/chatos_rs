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
    extract_tools, invalidate_stdio_session, jsonrpc_http_call, jsonrpc_http_tool_call_cancellable,
    jsonrpc_stdio_call, parse_tool_definition, McpStdioServer,
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

mod preparation;
mod validation;

pub(super) use validation::load_verified_manifest;
use validation::{validate_invocation_id, wait_for_invocation_cancellation};

const MAX_MANIFEST_BYTES: u64 = 1024 * 1024;
const MAX_MCP_TOOLS: usize = 200;
const MAX_MCP_TOOL_SNAPSHOT_BYTES: usize = 512 * 1024;
const MCP_TOOL_CALL_OPERATION: &str = "mcp_tools_call";
const MCP_HEALTH_CHECK_OPERATION: &str = "mcp_health_check";
const MCP_HEALTH_PROBE_INTERVAL: Duration = Duration::from_secs(60);
const MAX_ACTIVE_MCP_INVOCATIONS: usize = 64;
const MAX_INVOCATION_ID_BYTES: usize = 256;
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
        let checked_at = preparation::health_timestamp();
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
        let checked_at = preparation::health_timestamp();
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
        self.snapshot.checked_at = preparation::health_timestamp();
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
    active_invocations: Arc<Mutex<std::collections::HashMap<String, ActivePluginMcpInvocation>>>,
}

#[derive(Clone)]
struct ActivePluginMcpInvocation {
    cancellation: CancellationToken,
    identity: Arc<()>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum PluginMcpInvocationCancelOutcome {
    Cancelled,
    CancelRequested,
    InvocationNotFound,
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
        preparation::validate_active_mcp_snapshot(&self.installer, &self.snapshot)?;
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

    pub(super) async fn call_tool(
        &self,
        invocation_id: &str,
        tool_name: &str,
        arguments: Value,
    ) -> Result<Value> {
        validate_invocation_id(invocation_id)?;
        self.ensure_recent_health().await?;
        let active = ActivePluginMcpInvocation {
            cancellation: CancellationToken::new(),
            identity: Arc::new(()),
        };
        {
            let mut invocations = self
                .active_invocations
                .lock()
                .map_err(|_| anyhow!("Plugin MCP invocation state is unavailable"))?;
            if invocations.len() >= MAX_ACTIVE_MCP_INVOCATIONS {
                bail!("Plugin MCP active invocation capacity was reached");
            }
            if invocations.contains_key(invocation_id) {
                bail!("Plugin MCP invocation id is already active");
            }
            invocations.insert(invocation_id.to_string(), active.clone());
        }
        let result = self
            .invoker
            .call(
                &self.transport,
                "tools/call",
                json!({"name": tool_name, "arguments": arguments}),
                Some(active.cancellation.clone()),
            )
            .await;
        if let Ok(mut invocations) = self.active_invocations.lock() {
            if invocations
                .get(invocation_id)
                .is_some_and(|current| Arc::ptr_eq(&current.identity, &active.identity))
            {
                invocations.remove(invocation_id);
            }
        }
        result
    }

    pub(super) fn cancel(&self) {
        if let Ok(mut invocations) = self.active_invocations.lock() {
            for invocation in invocations.drain().map(|(_, invocation)| invocation) {
                invocation.cancellation.cancel();
            }
        }
        self.invoker.cancel(&self.transport);
    }

    pub(super) fn cancel_invocation(
        &self,
        invocation_id: &str,
    ) -> Result<PluginMcpInvocationCancelOutcome> {
        validate_invocation_id(invocation_id)?;
        let active = self
            .active_invocations
            .lock()
            .map_err(|_| anyhow!("Plugin MCP invocation state is unavailable"))?
            .remove(invocation_id);
        let Some(active) = active else {
            return Ok(PluginMcpInvocationCancelOutcome::InvocationNotFound);
        };
        active.cancellation.cancel();
        Ok(self
            .invoker
            .cancel_invocation(&self.transport, &active.cancellation))
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
            .call(&self.transport, "tools/list", json!({}), None)
            .await
        {
            Ok(response) => {
                preparation::sanitize_tools(response, &self.published_tools, &BTreeSet::new())
                    .and_then(|tools| preparation::sha256_json(&tools))
            }
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
        invocation_cancellation: Option<CancellationToken>,
    ) -> Result<Value>;

    fn cancel(&self, transport: &PreparedPluginMcpTransport);

    fn cancel_invocation(
        &self,
        transport: &PreparedPluginMcpTransport,
        cancellation: &CancellationToken,
    ) -> PluginMcpInvocationCancelOutcome;
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
        invocation_cancellation: Option<CancellationToken>,
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
                    biased;
                    _ = cancellation.cancelled() => {
                        bail!("Plugin MCP stdio request was cancelled")
                    }
                    _ = wait_for_invocation_cancellation(invocation_cancellation) => {
                        bail!("Plugin MCP stdio invocation was cancelled")
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
                    if method == "tools/call" {
                        jsonrpc_http_tool_call_cancellable(
                            url,
                            Some(headers.as_map()),
                            params,
                            Some(*timeout),
                        )
                        .await
                        .map_err(anyhow::Error::msg)
                    } else {
                        jsonrpc_http_call(
                            url,
                            Some(headers.as_map()),
                            method,
                            params,
                            Some(*timeout),
                        )
                        .await
                        .map_err(anyhow::Error::msg)
                    }
                };
                tokio::select! {
                    biased;
                    _ = cancellation.cancelled() => {
                        bail!("Plugin MCP HTTP request was cancelled")
                    }
                    _ = wait_for_invocation_cancellation(invocation_cancellation) => {
                        bail!("Plugin MCP HTTP invocation was cancelled")
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

    fn cancel_invocation(
        &self,
        transport: &PreparedPluginMcpTransport,
        cancellation: &CancellationToken,
    ) -> PluginMcpInvocationCancelOutcome {
        cancellation.cancel();
        match transport {
            PreparedPluginMcpTransport::Stdio { server, .. } => {
                invalidate_stdio_session(server);
                PluginMcpInvocationCancelOutcome::Cancelled
            }
            PreparedPluginMcpTransport::Http { .. } => {
                PluginMcpInvocationCancelOutcome::CancelRequested
            }
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
        preparation::validate_required_permissions(
            &installation,
            component_key,
            permission_snapshot,
        )?;
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
        let transport = preparation::prepare_transport(
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
            .call(&transport, "tools/list", json!({}), None)
            .await
            .context("discover Plugin MCP tools")?;
        let tools = preparation::sanitize_tools(response, tool_allowlist, tool_blocklist)?;
        let published_tools = tools
            .iter()
            .filter_map(parse_tool_definition)
            .map(|tool| tool.name)
            .collect::<BTreeSet<_>>();
        let tool_snapshot_sha256 = preparation::sha256_json(&tools)?;
        let transport_name = match &transport {
            PreparedPluginMcpTransport::Stdio { .. } => "stdio",
            PreparedPluginMcpTransport::Http { .. } => "http",
        }
        .to_string();
        let snapshot_sha256 = preparation::mcp_snapshot_sha256(
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
            active_invocations: Arc::new(Mutex::new(std::collections::HashMap::new())),
        })
    }
}
