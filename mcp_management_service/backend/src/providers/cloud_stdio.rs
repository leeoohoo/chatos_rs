// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::{BTreeMap, HashSet};
use std::time::Duration;

use chatos_mcp_management_sdk::{McpProviderKind, ResolvedMcpRoute, SandboxExecutionTarget};
use chatos_plugin_management_sdk::{PluginMcpCloudRuntimeBundle, ResolvedMcp};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::runtime::{CloudStdioProviderBinding, PluginMcpRuntimeBinding, RuntimeSessionSnapshot};

use super::{ProviderCallError, ProviderCallOutcome, ProviderCancelOutcome};

mod init;
mod manager_client;
mod plugin_binding;
mod prepare;
mod runtime_calls;
mod validation;
use validation::*;

const CALLER_SERVICE: &str = "mcp-management-service";
const TOKEN_AUDIENCE: &str = "sandbox-manager";
const INTERNAL_SCOPE: &str = "sandbox.service";
const MAX_COMMAND_BYTES: usize = 256;
const MAX_TOOL_POLICY_ITEMS: usize = 512;
const MAX_TOOL_NAME_BYTES: usize = 256;

#[derive(Clone)]
pub(super) struct CloudStdioProvider {
    http: reqwest::Client,
    base_url: String,
    internal_secret: Option<String>,
    request_timeout: Duration,
    response_limit_bytes: usize,
}

#[derive(Debug, Serialize)]
struct CloudStdioCallRequest<'a> {
    runtime_session_id: &'a str,
    resource_id: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    invocation_id: Option<&'a str>,
    command: &'a str,
    args: &'a [String],
    env: &'a BTreeMap<String, String>,
    cwd: Option<&'a str>,
    plugin_artifact: Option<&'a PluginMcpCloudRuntimeBundle>,
    plugin_workspace_write: bool,
    method: &'a str,
    params: Value,
    expires_at_unix: i64,
    timeout_ms: u64,
}

#[derive(Debug, Serialize)]
struct CloudStdioCloseRequest<'a> {
    runtime_session_id: &'a str,
    resource_id: &'a str,
}

#[derive(Debug, Serialize)]
struct CloudStdioCancelRequest<'a> {
    runtime_session_id: &'a str,
    resource_id: &'a str,
    invocation_id: &'a str,
}

#[derive(Debug, Deserialize)]
struct CloudStdioCallResponse {
    result: Value,
}

#[derive(Debug, Deserialize)]
struct CloudStdioCancelResponse {
    status: String,
}

impl CloudStdioProvider {}

#[cfg(test)]
mod tests;
