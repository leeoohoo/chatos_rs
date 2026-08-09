// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::{HashMap, HashSet};
use std::time::Duration;

use chatos_mcp_management_sdk::{
    McpProviderKind, ProjectExecutionContext, ResolvedMcpRoute, WorkspaceProviderKind,
};
use serde::Deserialize;
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::runtime::{PluginLocalProviderBinding, PluginMcpRuntimeBinding, RuntimeSessionSnapshot};

use super::{ProviderCallError, ProviderCallOutcome, ProviderCancelOutcome};

const CALLER_SERVICE: &str = "mcp-management-service";
const TOKEN_AUDIENCE: &str = "local-connector-service";
const PLUGIN_RELAY_SCOPE: &str = "plugin.execute";
const MCP_TOOL_CALL_OPERATION: &str = "mcp_tools_call";
const MAX_PLUGIN_TOOLS: usize = 200;
const MAX_PLUGIN_TOOL_SNAPSHOT_BYTES: usize = 512 * 1024;

#[path = "plugin_local/local_runtime.rs"]
mod local_runtime;
#[path = "plugin_local/relay_client.rs"]
mod relay_client;

#[derive(Clone)]
pub(super) struct PluginLocalProvider {
    http: reqwest::Client,
    base_url: String,
    internal_secret: Option<String>,
    request_timeout: Duration,
    response_limit_bytes: usize,
}

#[derive(Debug, Deserialize)]
struct PluginPrepareResponse {
    run_id: String,
    plugin_id: String,
    release_id: String,
    version: String,
    artifact_sha256: String,
    component_key: String,
    mcp: PreparedPluginMcpSnapshot,
    operations: Vec<String>,
    adapter_session_id: String,
    session_sha256: String,
    expires_at: i64,
}

#[derive(Debug, Deserialize)]
struct PreparedPluginMcpSnapshot {
    plugin_id: String,
    release_id: String,
    version: String,
    artifact_sha256: String,
    component_key: String,
    oauth_connection_id: Option<String>,
    tools: Vec<Value>,
    tool_snapshot_sha256: String,
}

#[derive(Debug, Deserialize)]
struct PluginExecuteResponse {
    plugin_id: String,
    release_id: String,
    version: String,
    artifact_sha256: String,
    component_key: String,
    invocation_id: String,
    tool_name: String,
    adapter_session_id: String,
    operation: String,
    result: Value,
}

#[derive(Debug, Deserialize)]
struct PluginCancelResponse {
    run_id: String,
    adapter_session_id: String,
    invocation_id: String,
    status: String,
}

impl PluginLocalProvider {
    pub(super) fn new(
        http: reqwest::Client,
        base_url: impl Into<String>,
        request_timeout: Duration,
        internal_secret: Option<String>,
        response_limit_bytes: usize,
    ) -> Result<Self, String> {
        let base_url = base_url.into();
        let parsed = reqwest::Url::parse(base_url.as_str())
            .map_err(|error| format!("Plugin Local Provider base URL is invalid: {error}"))?;
        if !matches!(parsed.scheme(), "http" | "https") {
            return Err("Plugin Local Provider base URL must use http or https".to_string());
        }
        Ok(Self {
            http,
            base_url: base_url.trim().trim_end_matches('/').to_string(),
            internal_secret: internal_secret
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty()),
            request_timeout,
            response_limit_bytes,
        })
    }

    pub(super) fn supports(&self, route: &ResolvedMcpRoute) -> bool {
        self.internal_secret.is_some()
            && route.provider_kind == McpProviderKind::PluginLocal
            && route
                .provider_ref
                .as_deref()
                .is_some_and(|value| value.starts_with("plugin-binding:"))
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) async fn prepare_routes(
        &self,
        immutable_bindings: &HashMap<String, PluginMcpRuntimeBinding>,
        routes: &mut [ResolvedMcpRoute],
        context: &ProjectExecutionContext,
        runtime_session_id: &str,
        owner_user_id: &str,
        expires_at_unix: i64,
    ) -> (
        HashMap<String, PluginLocalProviderBinding>,
        HashMap<String, Vec<Value>>,
    ) {
        let mut bindings = HashMap::new();
        let mut tool_snapshots = HashMap::new();
        for route in routes
            .iter_mut()
            .filter(|route| route.provider_kind == McpProviderKind::PluginLocal)
        {
            route.cancel_supported = false;
            let Some(immutable) = immutable_bindings.get(route.resource_id.as_str()) else {
                make_route_unavailable(route, "immutable Plugin MCP binding is missing");
                continue;
            };
            match self
                .prepare_route(
                    immutable,
                    route,
                    context,
                    runtime_session_id,
                    owner_user_id,
                    expires_at_unix,
                )
                .await
            {
                Ok(binding) => {
                    route.cancel_supported = true;
                    tool_snapshots.insert(route.resource_id.clone(), binding.tools.clone());
                    bindings.insert(route.resource_id.clone(), binding);
                }
                Err(error) => make_route_unavailable(route, error.message.as_str()),
            }
        }
        (bindings, tool_snapshots)
    }
}

fn validate_prepare_response(
    immutable: &PluginMcpRuntimeBinding,
    runtime_session_id: &str,
    runtime_expires_at_unix: i64,
    prepared: &PluginPrepareResponse,
) -> Result<(), ProviderCallError> {
    if prepared.run_id != runtime_session_id
        || prepared.plugin_id != immutable.plugin_id
        || prepared.release_id != immutable.release_id
        || prepared.version != immutable.version
        || prepared.artifact_sha256 != immutable.artifact_sha256
        || prepared.component_key != immutable.component_key
        || prepared.mcp.plugin_id != immutable.plugin_id
        || prepared.mcp.release_id != immutable.release_id
        || prepared.mcp.version != immutable.version
        || prepared.mcp.artifact_sha256 != immutable.artifact_sha256
        || prepared.mcp.component_key != immutable.component_key
    {
        return Err(ProviderCallError::invalid_response(
            "Plugin Local prepare response does not match the immutable runtime binding",
        ));
    }
    if prepared.adapter_session_id.trim().is_empty()
        || !is_lower_sha256(prepared.session_sha256.as_str())
        || !is_lower_sha256(prepared.mcp.tool_snapshot_sha256.as_str())
        || prepared.expires_at <= chrono::Utc::now().timestamp()
        || prepared.expires_at < runtime_expires_at_unix
    {
        return Err(ProviderCallError::invalid_response(
            "Plugin Local prepare returned an invalid or prematurely expiring session snapshot",
        ));
    }
    validate_tool_snapshot(
        prepared.mcp.tools.as_slice(),
        prepared.mcp.tool_snapshot_sha256.as_str(),
    )?;
    if prepared.mcp.oauth_connection_id.as_ref().is_some_and(|id| {
        !immutable
            .auth_connection_ids
            .iter()
            .any(|allowed| allowed == id)
    }) {
        return Err(ProviderCallError::invalid_response(
            "Plugin Local prepare selected an OAuth connection outside the immutable snapshot",
        ));
    }
    Ok(())
}

fn validate_tool_snapshot(tools: &[Value], expected_sha256: &str) -> Result<(), ProviderCallError> {
    if tools.is_empty() || tools.len() > MAX_PLUGIN_TOOLS {
        return Err(ProviderCallError::invalid_response(
            "Plugin MCP tool snapshot must contain between 1 and 200 tools",
        ));
    }
    let encoded = serde_json::to_vec(tools).map_err(|error| {
        ProviderCallError::invalid_response(format!(
            "serialize Plugin MCP tool snapshot failed: {error}"
        ))
    })?;
    if encoded.len() > MAX_PLUGIN_TOOL_SNAPSHOT_BYTES
        || hex::encode(Sha256::digest(encoded)) != expected_sha256
    {
        return Err(ProviderCallError::invalid_response(
            "Plugin MCP tool snapshot hash or size is invalid",
        ));
    }
    let mut names = HashSet::new();
    for tool in tools {
        let name = tool
            .get("name")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .ok_or_else(|| {
                ProviderCallError::invalid_response(
                    "Plugin MCP tool snapshot contains an unnamed tool",
                )
            })?;
        if !names.insert(name) {
            return Err(ProviderCallError::invalid_response(
                "Plugin MCP tool snapshot contains duplicate tool names",
            ));
        }
    }
    Ok(())
}

fn validate_bound_route(
    snapshot: &RuntimeSessionSnapshot,
    route: &ResolvedMcpRoute,
    binding: &PluginLocalProviderBinding,
) -> Result<(), ProviderCallError> {
    let immutable = snapshot
        .plugin_mcp_bindings
        .get(route.resource_id.as_str())
        .ok_or_else(|| {
            ProviderCallError::provider_unavailable(
                "immutable Plugin MCP runtime binding is missing",
            )
        })?;
    let workspace = snapshot.project_context.workspace.as_ref();
    if !matches!(
        snapshot.project_context.workspace_provider,
        WorkspaceProviderKind::LocalConnector
    ) || !snapshot
        .expires_at_unix
        .min(binding.expires_at_unix)
        .gt(&chrono::Utc::now().timestamp())
        || immutable != &binding.runtime
        || route.provider_ref.as_deref() != Some(binding.runtime.provider_ref.as_str())
        || route.allow_writes != binding.runtime.allow_writes
        || workspace.and_then(|workspace| workspace.device_id.as_deref())
            != Some(binding.device_id.as_str())
        || workspace.map(|workspace| workspace.workspace_id.as_str())
            != Some(binding.workspace_id.as_str())
    {
        return Err(ProviderCallError::provider_unavailable(
            "Plugin Local route does not match its prepared runtime session",
        ));
    }
    Ok(())
}

fn make_route_unavailable(route: &mut ResolvedMcpRoute, reason: &str) {
    route.provider_kind = McpProviderKind::Unavailable;
    route.provider_ref = None;
    route.allow_writes = false;
    route.cancel_supported = false;
    route.reason = format!("Plugin Local Provider unavailable: {reason}");
}

fn is_lower_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
}

#[cfg(test)]
mod tests;
