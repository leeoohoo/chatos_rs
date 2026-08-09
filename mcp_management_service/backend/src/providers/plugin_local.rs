// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::time::Duration;

use crate::runtime::PluginLocalProviderBinding;
use chatos_mcp_management_sdk::{McpProviderKind, ResolvedMcpRoute};
use serde::Deserialize;
use serde_json::Value;

const CALLER_SERVICE: &str = "mcp-management-service";
const TOKEN_AUDIENCE: &str = "local-connector-service";
const PLUGIN_RELAY_SCOPE: &str = "plugin.execute";
const MCP_TOOL_CALL_OPERATION: &str = "mcp_tools_call";
const MAX_PLUGIN_TOOLS: usize = 200;
const MAX_PLUGIN_TOOL_SNAPSHOT_BYTES: usize = 512 * 1024;

#[path = "plugin_local/local_runtime.rs"]
mod local_runtime;
#[path = "plugin_local/prepare.rs"]
mod prepare;
#[path = "plugin_local/relay_client.rs"]
mod relay_client;
#[path = "plugin_local/validation.rs"]
mod validation;

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
}

#[cfg(test)]
mod tests;
