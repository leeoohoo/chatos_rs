// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeSet;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::{anyhow, bail, Result};
use async_trait::async_trait;
use chatos_mcp_runtime::{
    invalidate_stdio_session, jsonrpc_http_call, jsonrpc_http_tool_call_cancellable,
    jsonrpc_stdio_call, McpStdioServer,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio_util::sync::CancellationToken;
use zeroize::Zeroize;

use super::super::super::oauth_broker::PluginOAuthTokenBinding;
use super::super::credentials::{
    PluginCredentialBindings, PluginHttpHeaderTemplates, PluginStdioEnvironmentTemplates,
};
use super::super::sandbox::PluginStdioSandboxRuntime;
use super::preparation;
use super::validation::{validate_invocation_id, wait_for_invocation_cancellation};
use super::{
    MAX_ACTIVE_MCP_INVOCATIONS, MCP_DEGRADED_STATUS, MCP_HEALTHY_STATUS,
    MCP_HEALTH_CHECK_OPERATION, MCP_HEALTH_PROBE_INTERVAL, MCP_TOOL_CALL_OPERATION,
};
use crate::plugins::PluginInstaller;

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
pub(in crate::plugins::runtime) struct PreparedPluginMcp {
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
pub(in crate::plugins::runtime) enum PluginMcpInvocationCancelOutcome {
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
    pub(in crate::plugins::runtime) fn snapshot(&self) -> &PluginMcpSnapshot {
        &self.snapshot
    }

    pub(in crate::plugins::runtime) fn operation(&self) -> &'static str {
        MCP_TOOL_CALL_OPERATION
    }

    pub(in crate::plugins::runtime) fn health_operation(&self) -> &'static str {
        MCP_HEALTH_CHECK_OPERATION
    }

    pub(in crate::plugins::runtime) fn publishes_tool(&self, tool_name: &str) -> bool {
        self.published_tools.contains(tool_name)
    }

    pub(in crate::plugins::runtime) fn validate_active(&self) -> Result<()> {
        preparation::validate_active_mcp_snapshot(&self.installer, &self.snapshot)?;
        self.transport.verify_bindings()
    }

    pub(in crate::plugins::runtime) fn health_snapshot(&self) -> Result<PluginMcpHealthSnapshot> {
        self.health
            .lock()
            .map(|health| health.snapshot.clone())
            .map_err(|_| anyhow!("Plugin MCP health state is unavailable"))
    }

    #[cfg(test)]
    pub(in crate::plugins::runtime) fn expire_health_probe_for_tests(&self) {
        if let Ok(mut health) = self.health.lock() {
            health.checked_at = Instant::now() - MCP_HEALTH_PROBE_INTERVAL;
        }
    }

    pub(in crate::plugins::runtime) async fn check_health(
        &self,
    ) -> Result<PluginMcpHealthSnapshot> {
        let _probe_guard = self.health_probe_lock.lock().await;
        self.probe_health().await
    }

    pub(in crate::plugins::runtime) async fn call_tool(
        &self,
        invocation_id: &str,
        tool_name: &str,
        arguments: Value,
        tool_result_max_chars: Option<usize>,
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
        let mut params = json!({"name": tool_name, "arguments": arguments});
        if let Some(max_chars) = tool_result_max_chars.filter(|value| {
            (1..=chatos_mcp_service::TOOL_RESULT_MAX_CHARS_UPPER_BOUND).contains(value)
        }) {
            params["_meta"] = json!({
                chatos_mcp_service::TOOL_RESULT_MAX_CHARS_META_KEY: max_chars,
            });
        }
        let result = self
            .invoker
            .call(
                &self.transport,
                "tools/call",
                params,
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

    pub(in crate::plugins::runtime) fn cancel(&self) {
        if let Ok(mut invocations) = self.active_invocations.lock() {
            for invocation in invocations.drain().map(|(_, invocation)| invocation) {
                invocation.cancellation.cancel();
            }
        }
        self.invoker.cancel(&self.transport);
    }

    pub(in crate::plugins::runtime) fn cancel_invocation(
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

    pub(super) fn new(
        snapshot: PluginMcpSnapshot,
        transport: PreparedPluginMcpTransport,
        published_tools: BTreeSet<String>,
        invoker: Arc<dyn PluginMcpInvoker>,
        installer: PluginInstaller,
    ) -> Self {
        Self {
            snapshot,
            transport,
            published_tools,
            invoker,
            installer,
            health: Arc::new(Mutex::new(PluginMcpHealthState::healthy())),
            health_probe_lock: Arc::new(tokio::sync::Mutex::new(())),
            active_invocations: Arc::new(Mutex::new(std::collections::HashMap::new())),
        }
    }
}

#[derive(Debug, Clone)]
pub(in crate::plugins::runtime) enum PreparedPluginMcpTransport {
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
    pub(super) fn credential_snapshot_sha256(&self) -> Option<&str> {
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

    pub(super) fn oauth_snapshot_sha256(&self) -> Option<&str> {
        match self {
            Self::Stdio { .. } => None,
            Self::Http { oauth_binding, .. } => oauth_binding
                .as_deref()
                .map(PluginOAuthTokenBinding::snapshot_sha256),
        }
    }

    pub(super) fn oauth_connection_id(&self) -> Option<&str> {
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
pub(in crate::plugins::runtime) trait PluginMcpInvoker:
    Send + Sync
{
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
pub(super) struct DefaultPluginMcpInvoker;

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
                            chatos_mcp_runtime::McpAsyncResultTransport::Disabled,
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
