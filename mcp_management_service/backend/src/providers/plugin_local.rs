// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use crate::runtime::PluginLocalProviderBinding;
use serde::Deserialize;
use serde_json::Value;
use tokio::sync::{Mutex, RwLock};

const CALLER_SERVICE: &str = "mcp-management-service";
const TOKEN_AUDIENCE: &str = "local-connector-service";
const PLUGIN_RELAY_SCOPE: &str = "plugin.execute";
const MCP_TOOL_CALL_OPERATION: &str = "mcp_tools_call";
const MAX_PLUGIN_TOOLS: usize = 200;
const MAX_PLUGIN_TOOL_SNAPSHOT_BYTES: usize = 512 * 1024;
const MAX_PLUGIN_SERVER_INSTRUCTIONS_BYTES: usize = 64 * 1024;

mod init;
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
    recovered_bindings: Arc<RwLock<HashMap<String, PluginLocalProviderBinding>>>,
    recovery_lock: Arc<Mutex<()>>,
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
    server_instructions: Option<String>,
    server_instructions_sha256: String,
    tools: Vec<Value>,
    tool_snapshot_sha256: String,
    snapshot_sha256: String,
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

#[cfg(test)]
mod tests;
