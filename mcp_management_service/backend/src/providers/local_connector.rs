// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::time::Duration;

use super::ProviderCallError;

mod binding;
mod init;
mod lifecycle;
mod request_builder;
mod runtime_calls;

const CALLER_SERVICE: &str = "mcp-management-service";
const TOKEN_AUDIENCE: &str = "local-connector-service";
const MCP_RELAY_SCOPE: &str = "relay.mcp";
const LOCAL_CONNECTOR_PROJECT_ID_HEADER: &str = "x-local-connector-project-id";
const LOCAL_CONNECTOR_DEFAULT_TOOL_ROOT_HEADER: &str = "x-local-connector-default-tool-root";
const MCP_MANAGEMENT_SESSION_ID_HEADER: &str = "x-mcp-management-session-id";
const MCP_MANAGEMENT_RUN_ID_HEADER: &str = "x-mcp-management-run-id";
const MCP_MANAGEMENT_EXECUTION_GROUP_ID_HEADER: &str = "x-mcp-management-execution-group-id";
const MCP_MANAGEMENT_SCOPE_GENERATION_HEADER: &str = "x-mcp-management-execution-scope-generation";
const MCP_MANAGEMENT_TASK_ID_HEADER: &str = "x-mcp-management-task-id";
const MCP_MANAGEMENT_SESSION_EXPIRES_AT_UNIX_HEADER: &str =
    "x-mcp-management-session-expires-at-unix";
const TERMINAL_WAIT_TRANSPORT_GRACE_MS: u64 = 30_000;

#[derive(Clone)]
pub(super) struct LocalConnectorProvider {
    http: reqwest::Client,
    base_url: String,
    internal_secret: Option<String>,
    request_timeout: Duration,
    response_limit_bytes: usize,
}

fn local_connector_call_timeout(
    original_tool_name: &str,
    arguments: &serde_json::Value,
    default_timeout: Duration,
) -> Duration {
    let tool_name = original_tool_name.trim();
    let is_terminal_wait = tool_name == "process_wait"
        || tool_name.ends_with("_process_wait")
        || ((tool_name == "process" || tool_name.ends_with("_process"))
            && arguments.get("action").and_then(serde_json::Value::as_str) == Some("wait"));
    if !is_terminal_wait {
        return default_timeout;
    }
    let requested_timeout_ms = chatos_mcp::resolve_wait_timeout_ms(arguments);
    default_timeout.max(Duration::from_millis(
        requested_timeout_ms.saturating_add(TERMINAL_WAIT_TRANSPORT_GRACE_MS),
    ))
}

#[cfg(test)]
mod tests;
