// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashMap;
use std::time::Duration;

use chatos_mcp_management_sdk::{
    CreateRuntimeSessionRequest, McpManagementClient, McpManagementClientConfig,
    McpManagementRuntimeSessionHandle, RuntimeSessionResponse, SandboxExecutionTarget,
};
use chatos_mcp_runtime::McpHttpServer;
use chatos_plugin_management_sdk::SystemMcpKey;
use tracing::{info, warn};

use crate::models::{TaskRecord, TaskRunRecord};

use crate::services::sandbox_runtime::SandboxRuntimeContext;

const GATEWAY_SERVER_NAME: &str = "mcp_management";
const DEFAULT_TOOL_TIMEOUT_MS: u64 = 180_000;
const ASK_USER_TRANSPORT_TIMEOUT_MS: u64 =
    chatos_mcp::ASK_USER_PROMPT_TIMEOUT_MS_DEFAULT + 5 * 60 * 1_000;
const TERMINAL_WAIT_TRANSPORT_TIMEOUT_MS: u64 = chatos_mcp::PROCESS_WAIT_MAX_TIMEOUT_MS + 15_000;

pub(super) async fn resolve_mcp_management_gateway(
    task: &TaskRecord,
    run: &TaskRunRecord,
    sandbox_context: Option<&SandboxRuntimeContext>,
) -> Result<ResolvedMcpManagementGateway, String> {
    let owner_user_id = normalized_task_owner_user_id(task)
        .ok_or_else(|| "task owner user id is required for MCP Management".to_string())?;
    let agent_key = crate::models::task_runner_agent_key_for(
        task.task_profile.as_str(),
        task.mcp_config.requires_execution,
    );
    let config = McpManagementClientConfig::from_env("task-runner").await;
    let client = McpManagementClient::new(config)
        .map_err(|err| format!("initialize MCP Management client failed: {err}"))?;
    let sandbox_provider = sandbox_context
        .map(SandboxRuntimeContext::provider_kind)
        .transpose()?;
    let request = CreateRuntimeSessionRequest {
        owner_user_id,
        agent_key: agent_key.as_str().to_string(),
        project_id: crate::models::normalize_project_id(Some(task.project_id.clone())),
        run_id: Some(run.id.clone()),
        turn_id: None,
        task_id: Some(task.id.clone()),
        task_profile: Some(task.task_profile.clone()),
        source_session_id: task.source_session_id.clone(),
        source_user_message_id: task.source_user_message_id.clone(),
        contact_agent_id: None,
        default_model_config_id: task.default_model_config_id.clone(),
        expected_project_task_ids: Vec::new(),
        locale: Some(if task.mcp_config.locale().is_english() {
            "en-US".to_string()
        } else {
            "zh-CN".to_string()
        }),
        requested_device_id: None,
        requested_sandbox_provider: sandbox_provider,
        sandbox_target: sandbox_context.map(|context| SandboxExecutionTarget {
            provider: sandbox_provider.expect("sandbox context provider was resolved"),
            pairing_id: context.local_connector_pairing_id.clone(),
            sandbox_id: context.sandbox_id.clone(),
            lease_id: context.lease_id.clone(),
            is_environment: context.is_environment,
            service_id: context.service_id.clone(),
        }),
    };
    let session = client
        .resolve_runtime_session(&request)
        .await
        .map_err(|err| format!("resolve MCP Management runtime session failed: {err}"))?;
    info!(
        task_id = task.id.as_str(),
        run_id = run.id.as_str(),
        session_id = session.session_id.as_str(),
        route_revision = session.route_revision.as_str(),
        configured_mcp_count = session.configured_mcp_count,
        exposed_tool_count = session.exposed_tool_count,
        "Task Runner resolved MCP Management runtime session"
    );
    let timeout = Duration::from_millis(
        std::env::var("TASK_RUNNER_MCP_MANAGEMENT_TOOL_TIMEOUT_MS")
            .ok()
            .and_then(|value| value.parse::<u64>().ok())
            .unwrap_or(DEFAULT_TOOL_TIMEOUT_MS)
            .clamp(1_000, 2 * 60 * 60 * 1_000),
    );
    let ask_user_timeout = Duration::from_millis(
        std::env::var("TASK_RUNNER_MCP_MANAGEMENT_ASK_USER_TOOL_TIMEOUT_MS")
            .ok()
            .and_then(|value| value.parse::<u64>().ok())
            .unwrap_or(ASK_USER_TRANSPORT_TIMEOUT_MS)
            .clamp(
                chatos_mcp::ASK_USER_PROMPT_TIMEOUT_MS_DEFAULT,
                7 * 24 * 60 * 60 * 1_000,
            ),
    );
    let provider_skills_prompt = session.provider_skills_prompt.clone();
    let runtime_session =
        McpManagementRuntimeSessionHandle::new(client, session.session_id.clone());
    let server = match gateway_server(session, timeout, ask_user_timeout) {
        Ok(server) => server,
        Err(error) => {
            let mcp_session_id = runtime_session.session_id().to_string();
            if let Err(close_error) = runtime_session.close().await {
                warn!(
                    task_id = task.id.as_str(),
                    run_id = run.id.as_str(),
                    mcp_session_id,
                    error = %close_error,
                    "close invalid Task Runner MCP Management runtime session failed"
                );
            }
            return Err(error);
        }
    };
    Ok(ResolvedMcpManagementGateway {
        server,
        provider_skills_prompt,
        runtime_session,
    })
}

pub(super) struct ResolvedMcpManagementGateway {
    server: McpHttpServer,
    pub(super) provider_skills_prompt: Option<String>,
    runtime_session: McpManagementRuntimeSessionHandle,
}

impl ResolvedMcpManagementGateway {
    pub(super) fn into_parts(self) -> (McpHttpServer, McpManagementRuntimeSessionHandle) {
        (self.server, self.runtime_session)
    }
}

fn gateway_server(
    session: RuntimeSessionResponse,
    timeout: Duration,
    ask_user_timeout: Duration,
) -> Result<McpHttpServer, String> {
    let mut server = McpHttpServer::new(GATEWAY_SERVER_NAME, session.mcp_server_url)
        .with_headers(HashMap::from([(
            "authorization".to_string(),
            format!("Bearer {}", session.runtime_token),
        )]))
        .with_timeout(timeout)
        .with_preserved_tool_names()
        .with_fail_on_unavailable();
    let descriptor = chatos_mcp::system_mcp_descriptor(SystemMcpKey::AskUser);
    for tool in chatos_mcp::system_mcp_static_tools(SystemMcpKey::AskUser)? {
        let Some(name) = tool.get("name").and_then(serde_json::Value::as_str) else {
            return Err("Ask User MCP tool catalog contains a tool without a name".to_string());
        };
        server = server.with_tool_timeout(
            format!("{}_{}", descriptor.server_name, name.trim()),
            ask_user_timeout,
        );
    }
    let terminal_descriptor = chatos_mcp::system_mcp_descriptor(SystemMcpKey::TerminalController);
    for tool_name in ["process_wait", "process"] {
        server = server.with_tool_timeout(
            format!("{}_{}", terminal_descriptor.server_name, tool_name),
            Duration::from_millis(TERMINAL_WAIT_TRANSPORT_TIMEOUT_MS),
        );
    }
    Ok(server)
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
                effective_mcp_ids: Vec::new(),
                provider_skills_prompt: None,
                unavailable_required_mcps: Vec::new(),
            },
            Duration::from_secs(30),
            Duration::from_secs(3_600),
        )
        .expect("gateway server");
        assert!(server.preserve_tool_names);
        assert!(server.fail_on_unavailable);
        assert_eq!(server.timeout_duration(), Some(Duration::from_secs(30)));
        assert_eq!(
            server.tool_timeout_duration("ask_user_prompt_choices"),
            Some(Duration::from_secs(3_600))
        );
        assert_eq!(
            server.tool_timeout_duration("terminal_controller_process_wait"),
            Some(Duration::from_millis(TERMINAL_WAIT_TRANSPORT_TIMEOUT_MS))
        );
        assert_eq!(
            server.tool_timeout_duration("terminal_controller_process"),
            Some(Duration::from_millis(TERMINAL_WAIT_TRANSPORT_TIMEOUT_MS))
        );
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
