// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeSet;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{bail, Context, Result};
use chatos_mcp_runtime::parse_tool_definition;
use chatos_plugin_management_sdk::PluginComponentKind;
use serde_json::json;

use super::super::oauth_broker::PluginOAuthBroker;
use crate::plugins::{PluginCredentialVault, PluginInstaller};

mod preparation;
mod runtime_shell;
mod validation;

use runtime_shell::DefaultPluginMcpInvoker;
pub use runtime_shell::{PluginMcpHealthSnapshot, PluginMcpSnapshot};
pub(in crate::plugins::runtime) use runtime_shell::{
    PluginMcpInvocationCancelOutcome, PluginMcpInvoker, PreparedPluginMcp,
    PreparedPluginMcpTransport,
};
pub(in crate::plugins::runtime) use validation::load_verified_manifest;

const MAX_MCP_TOOLS: usize = 200;
const MAX_MCP_TOOL_SNAPSHOT_BYTES: usize = 512 * 1024;
const MCP_TOOL_CALL_OPERATION: &str = "mcp_tools_call";
const MCP_HEALTH_CHECK_OPERATION: &str = "mcp_health_check";
const MCP_HEALTH_PROBE_INTERVAL: Duration = Duration::from_secs(60);
const MAX_ACTIVE_MCP_INVOCATIONS: usize = 64;
const MAX_INVOCATION_ID_BYTES: usize = 256;
const MCP_HEALTHY_STATUS: &str = "healthy";
const MCP_DEGRADED_STATUS: &str = "degraded";

#[derive(Clone)]
pub struct PluginMcpAdapter {
    installer: PluginInstaller,
    credential_vault: Option<PluginCredentialVault>,
    oauth_broker: Option<PluginOAuthBroker>,
    invoker: Arc<dyn PluginMcpInvoker>,
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
        Self {
            installer,
            credential_vault,
            oauth_broker: None,
            invoker: Arc::new(DefaultPluginMcpInvoker),
        }
    }

    #[cfg(test)]
    pub(in crate::plugins::runtime) fn with_invoker(
        installer: PluginInstaller,
        invoker: Arc<dyn PluginMcpInvoker>,
    ) -> Self {
        let credential_vault = installer.credential_vault();
        Self {
            installer,
            credential_vault,
            oauth_broker: None,
            invoker,
        }
    }

    #[cfg(test)]
    pub(in crate::plugins::runtime) fn with_stdio_execution_for_tests(
        installer: PluginInstaller,
    ) -> Self {
        let credential_vault = installer.credential_vault();
        Self {
            installer,
            credential_vault,
            oauth_broker: None,
            invoker: Arc::new(DefaultPluginMcpInvoker),
        }
    }

    pub fn with_oauth_broker(mut self, oauth_broker: PluginOAuthBroker) -> Self {
        self.oauth_broker = Some(oauth_broker);
        self
    }

    pub(in crate::plugins::runtime) async fn prepare(
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
        if requested_server_key.is_some_and(|server_key| server_key != component_key) {
            bail!("Plugin MCP server_key must match component_key");
        }
        let server = declared_server.clone();
        let server_key = server.component_key().to_string();
        let transport = preparation::prepare_transport(
            &installation,
            &server,
            adapter_session_id,
            owner_user_id,
            device_id,
            component_key,
            permission_snapshot,
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
        Ok(PreparedPluginMcp::new(
            PluginMcpSnapshot {
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
            self.invoker.clone(),
            self.installer.clone(),
        ))
    }
}
