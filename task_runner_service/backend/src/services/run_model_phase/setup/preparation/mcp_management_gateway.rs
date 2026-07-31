// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashMap;
use std::time::Duration;

use chatos_mcp_management_sdk::{
    CreateRuntimeSessionRequest, McpManagementClient, McpManagementClientConfig,
    RuntimeSessionResponse, SandboxExecutionTarget, SandboxProviderKind,
};
use chatos_mcp_runtime::McpHttpServer;
use tracing::{info, warn};

use crate::models::{TaskRecord, TaskRunRecord};

use crate::services::sandbox_runtime::SandboxRuntimeContext;

const GATEWAY_SERVER_NAME: &str = "mcp_management";
const DEFAULT_TOOL_TIMEOUT_MS: u64 = 180_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum McpManagementExecutionMode {
    Off,
    Shadow,
    Gateway,
}

impl McpManagementExecutionMode {
    fn from_value(value: Option<&str>) -> Self {
        match value
            .map(str::trim)
            .unwrap_or_default()
            .to_ascii_lowercase()
            .as_str()
        {
            "shadow" | "observe" => Self::Shadow,
            "gateway" | "enabled" | "on" | "true" | "1" => Self::Gateway,
            _ => Self::Off,
        }
    }

    fn from_env() -> Self {
        Self::from_value(
            std::env::var("TASK_RUNNER_MCP_MANAGEMENT_MODE")
                .ok()
                .as_deref(),
        )
    }
}

pub(super) async fn resolve_mcp_management_gateway(
    task: &TaskRecord,
    run: &TaskRunRecord,
    sandbox_context: Option<&SandboxRuntimeContext>,
) -> Result<Option<McpHttpServer>, String> {
    let mode = McpManagementExecutionMode::from_env();
    if mode == McpManagementExecutionMode::Off {
        return Ok(None);
    }
    let owner_user_id = normalized_task_owner_user_id(task)
        .ok_or_else(|| "task owner user id is required for MCP Management".to_string())?;
    let agent_key = crate::models::task_runner_agent_key_for(
        task.task_profile.as_str(),
        task.mcp_config.requires_execution,
    );
    let config = McpManagementClientConfig::from_env("task-runner").await;
    let client = McpManagementClient::new(config.clone())
        .map_err(|err| format!("initialize MCP Management client failed: {err}"))?;
    let request = CreateRuntimeSessionRequest {
        owner_user_id,
        agent_key: agent_key.as_str().to_string(),
        project_id: crate::models::normalize_project_id(Some(task.project_id.clone())),
        run_id: Some(run.id.clone()),
        turn_id: None,
        task_id: Some(task.id.clone()),
        task_profile: Some(task.task_profile.clone()),
        requested_device_id: None,
        requested_sandbox_provider: sandbox_context.map(|_| SandboxProviderKind::Cloud),
        sandbox_target: sandbox_context.map(|context| SandboxExecutionTarget {
            sandbox_id: context.sandbox_id.clone(),
            lease_id: context.lease_id.clone(),
            is_environment: context.is_environment,
            service_id: context.service_id.clone(),
        }),
    };
    let session = match client.resolve_runtime_session(&request).await {
        Ok(session) => session,
        Err(err) if mode == McpManagementExecutionMode::Shadow => {
            warn!(
                task_id = task.id.as_str(),
                run_id = run.id.as_str(),
                error = %err,
                "MCP Management shadow session resolution failed; legacy execution remains active"
            );
            return Ok(None);
        }
        Err(err) => {
            return Err(format!(
                "resolve MCP Management runtime session failed: {err}"
            ));
        }
    };
    info!(
        task_id = task.id.as_str(),
        run_id = run.id.as_str(),
        session_id = session.session_id.as_str(),
        route_revision = session.route_revision.as_str(),
        configured_mcp_count = session.configured_mcp_count,
        exposed_tool_count = session.exposed_tool_count,
        execution_mode = ?mode,
        "Task Runner resolved MCP Management runtime session"
    );
    if mode == McpManagementExecutionMode::Shadow {
        return Ok(None);
    }
    let timeout = Duration::from_millis(
        std::env::var("TASK_RUNNER_MCP_MANAGEMENT_TOOL_TIMEOUT_MS")
            .ok()
            .and_then(|value| value.parse::<u64>().ok())
            .unwrap_or(DEFAULT_TOOL_TIMEOUT_MS)
            .clamp(1_000, 2 * 60 * 60 * 1_000),
    );
    Ok(Some(gateway_server(session, timeout)))
}

fn gateway_server(session: RuntimeSessionResponse, timeout: Duration) -> McpHttpServer {
    McpHttpServer::new(GATEWAY_SERVER_NAME, session.mcp_server_url)
        .with_headers(HashMap::from([(
            "authorization".to_string(),
            format!("Bearer {}", session.runtime_token),
        )]))
        .with_timeout(timeout)
        .with_preserved_tool_names()
        .with_fail_on_unavailable()
}

fn normalized_task_owner_user_id(task: &TaskRecord) -> Option<String> {
    task.owner_user_id
        .as_deref()
        .or(task.creator_user_id.as_deref())
        .or(Some(task.subject_id.as_str()))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn execution_mode_is_explicit_and_fail_closed() {
        assert_eq!(
            McpManagementExecutionMode::from_value(Some("shadow")),
            McpManagementExecutionMode::Shadow
        );
        assert_eq!(
            McpManagementExecutionMode::from_value(Some("gateway")),
            McpManagementExecutionMode::Gateway
        );
        assert_eq!(
            McpManagementExecutionMode::from_value(Some("unexpected")),
            McpManagementExecutionMode::Off
        );
        assert_eq!(
            McpManagementExecutionMode::from_value(None),
            McpManagementExecutionMode::Off
        );
    }

    #[test]
    fn gateway_server_uses_runtime_grant_and_preserves_aggregated_names() {
        let server = gateway_server(
            RuntimeSessionResponse {
                session_id: "session-1".to_string(),
                policy_revision: "policy-1".to_string(),
                route_revision: "route-1".to_string(),
                expires_at: "2099-01-01T00:00:00Z".to_string(),
                mcp_server_url: "http://127.0.0.1:39280/mcp".to_string(),
                runtime_token: "runtime-token".to_string(),
                configured_mcp_count: 1,
                exposed_tool_count: 1,
                unavailable_required_mcps: Vec::new(),
            },
            Duration::from_secs(30),
        );
        assert!(server.preserve_tool_names);
        assert!(server.fail_on_unavailable);
        assert_eq!(server.timeout_duration(), Some(Duration::from_secs(30)));
        assert_eq!(
            server
                .headers
                .as_ref()
                .and_then(|headers| headers.get("authorization"))
                .map(String::as_str),
            Some("Bearer runtime-token")
        );
    }
}
