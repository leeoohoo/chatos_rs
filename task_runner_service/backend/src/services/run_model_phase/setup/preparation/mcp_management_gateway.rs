// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::time::Duration;

use chatos_mcp_gateway::McpManagementGatewayBuilder;
use chatos_mcp_management_sdk::{
    CreateRuntimeSessionRequest, McpManagementRuntimeSessionHandle, SandboxExecutionTarget,
};
use chatos_mcp_runtime::McpHttpServer;
use chatos_plugin_management_sdk::SystemMcpKey;
use tracing::info;

use crate::models::{TaskRecord, TaskRunRecord};

use crate::services::sandbox_runtime::SandboxRuntimeContext;

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
    let sandbox_provider = sandbox_context
        .map(SandboxRuntimeContext::provider_kind)
        .transpose()?;
    let request = CreateRuntimeSessionRequest {
        tenant_id: task.tenant_id.trim().to_string(),
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
    let mut builder = McpManagementGatewayBuilder::new("task-runner", request, timeout)
        .with_async_result_transport(chatos_mcp_runtime::McpAsyncResultTransport::RabbitMq);
    let descriptor = chatos_mcp::system_mcp_descriptor(SystemMcpKey::AskUser);
    for tool in chatos_mcp::system_mcp_static_tools(SystemMcpKey::AskUser)? {
        let Some(name) = tool.get("name").and_then(serde_json::Value::as_str) else {
            return Err("Ask User MCP tool catalog contains a tool without a name".to_string());
        };
        builder = builder.with_tool_timeout(
            format!("{}_{}", descriptor.server_name, name.trim()),
            ask_user_timeout,
        );
    }
    let terminal_descriptor = chatos_mcp::system_mcp_descriptor(SystemMcpKey::TerminalController);
    for tool_name in ["process_wait", "process"] {
        builder = builder.with_tool_timeout(
            format!("{}_{}", terminal_descriptor.server_name, tool_name),
            Duration::from_millis(TERMINAL_WAIT_TRANSPORT_TIMEOUT_MS),
        );
    }
    let resolved = builder
        .resolve()
        .await
        .map_err(|error| format!("resolve Task Runner MCP gateway failed: {error}"))?;
    info!(
        task_id = task.id.as_str(),
        run_id = run.id.as_str(),
        session_id = resolved.session_id.as_str(),
        route_revision = resolved.route_revision.as_str(),
        configured_mcp_count = resolved.configured_mcp_count,
        exposed_tool_count = resolved.exposed_tool_count,
        "Task Runner resolved MCP Management runtime session"
    );
    Ok(ResolvedMcpManagementGateway {
        server: resolved.server,
        provider_skills_prompt: resolved.provider_skills_prompt,
        runtime_session: resolved.runtime_session,
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

fn normalized_task_owner_user_id(task: &TaskRecord) -> Option<String> {
    task.owner_user_id
        .as_deref()
        .or(task.creator_user_id.as_deref())
        .or(Some(task.subject_id.as_str()))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}
