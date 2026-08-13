// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::time::Duration;

use chatos_agent::ChatosAgentProfile;
use chatos_mcp_gateway::McpManagementGatewayBuilder;
use chatos_mcp_management_sdk::{CreateRuntimeSessionRequest, McpManagementRuntimeSessionHandle};
use chatos_plugin_management_sdk::{PluginCommandInvocation, SelectedPluginRef, SystemMcpKey};
use tracing::{info, warn};

use crate::services::mcp_loader::McpHttpServer;

const DEFAULT_TOOL_TIMEOUT_MS: u64 = 180_000;
const ASK_USER_TRANSPORT_TIMEOUT_MS: u64 =
    chatos_mcp::ASK_USER_PROMPT_TIMEOUT_MS_DEFAULT + 5 * 60 * 1_000;

pub(super) struct McpManagementGatewayRequest<'a> {
    pub(super) tenant_id: Option<&'a str>,
    pub(super) owner_user_id: Option<&'a str>,
    pub(super) owner_role: Option<&'a str>,
    pub(super) agent_profile: ChatosAgentProfile,
    pub(super) project_id: Option<&'a str>,
    pub(super) source_session_id: Option<&'a str>,
    pub(super) turn_id: Option<&'a str>,
    pub(super) source_user_message_id: Option<&'a str>,
    pub(super) contact_agent_id: Option<&'a str>,
    pub(super) default_model_config_id: Option<&'a str>,
    pub(super) expected_project_task_ids: &'a [String],
    pub(super) selected_plugins: Vec<SelectedPluginRef>,
    pub(super) plugin_command_invocations: Vec<PluginCommandInvocation>,
    pub(super) locale: Option<&'a str>,
}

pub(super) struct McpManagementGateway {
    server: McpHttpServer,
    effective_mcp_ids: Vec<String>,
    provider_skills_prompt: Option<String>,
    plugin_instruction_items: Vec<serde_json::Value>,
    mcp_command_queue: String,
    runtime_session: McpManagementRuntimeSessionHandle,
}

impl McpManagementGateway {
    pub(super) fn into_parts(
        self,
    ) -> (
        McpHttpServer,
        Vec<String>,
        Option<String>,
        Vec<serde_json::Value>,
        String,
        McpManagementRuntimeSessionHandle,
    ) {
        (
            self.server,
            self.effective_mcp_ids,
            self.provider_skills_prompt,
            self.plugin_instruction_items,
            self.mcp_command_queue,
            self.runtime_session,
        )
    }
}

pub(super) async fn resolve_mcp_management_gateway(
    request: McpManagementGatewayRequest<'_>,
) -> Result<McpManagementGateway, String> {
    let tenant_id = required_text(request.tenant_id, "tenant_id")?;
    let owner_user_id = required_text(request.owner_user_id, "owner_user_id")?;
    let project_id = required_text(request.project_id, "project_id")?;
    let source_session_id = required_text(request.source_session_id, "source_session_id")?;
    let turn_id = required_text(request.turn_id, "turn_id")?;
    let source_user_message_id =
        required_text(request.source_user_message_id, "source_user_message_id")?;
    let agent_key = request.agent_profile.key().as_str().to_string();
    let session_request = CreateRuntimeSessionRequest {
        tenant_id: tenant_id.to_string(),
        owner_user_id: owner_user_id.to_string(),
        owner_role: normalized(request.owner_role),
        agent_key: agent_key.clone(),
        project_id: project_id.to_string(),
        run_id: None,
        turn_id: Some(turn_id.to_string()),
        task_id: None,
        task_profile: request
            .agent_profile
            .task_runner_task_profile()
            .map(ToOwned::to_owned),
        source_session_id: Some(source_session_id.to_string()),
        source_user_message_id: Some(source_user_message_id.to_string()),
        contact_agent_id: normalized(request.contact_agent_id),
        default_model_config_id: normalized(request.default_model_config_id),
        tool_result_max_chars: None,
        expected_project_task_ids: normalized_unique(request.expected_project_task_ids),
        requested_mcp_ids: None,
        selected_plugins: request.selected_plugins,
        plugin_command_invocations: request.plugin_command_invocations,
        locale: normalized(request.locale),
        requested_device_id: None,
        requested_sandbox_provider: None,
        sandbox_target: None,
    };
    let resolved = configured_gateway_builder(session_request)?
        .resolve()
        .await
        .map_err(|error| format!("resolve ChatOS MCP gateway failed: {error}"))?;
    info!(
        source_session_id,
        turn_id,
        agent_key = agent_key.as_str(),
        session_id = resolved.session_id.as_str(),
        route_revision = resolved.route_revision.as_str(),
        configured_mcp_count = resolved.configured_mcp_count,
        exposed_tool_count = resolved.exposed_tool_count,
        "ChatOS resolved MCP Management runtime session"
    );
    build_resolved_gateway(resolved, source_session_id, turn_id).await
}

pub(super) async fn resolve_existing_mcp_management_gateway(
    session_id: &str,
) -> Result<McpManagementGateway, String> {
    let session_id = required_text(Some(session_id), "session_id")?;
    let builder = configured_gateway_builder(CreateRuntimeSessionRequest {
        tenant_id: String::new(),
        owner_user_id: String::new(),
        owner_role: None,
        agent_key: String::new(),
        project_id: String::new(),
        run_id: None,
        turn_id: None,
        task_id: None,
        task_profile: None,
        source_session_id: None,
        source_user_message_id: None,
        contact_agent_id: None,
        default_model_config_id: None,
        tool_result_max_chars: None,
        expected_project_task_ids: Vec::new(),
        requested_mcp_ids: None,
        selected_plugins: Vec::new(),
        plugin_command_invocations: Vec::new(),
        locale: None,
        requested_device_id: None,
        requested_sandbox_provider: None,
        sandbox_target: None,
    })?;
    let resolved = builder.resolve_existing(session_id).await?;
    build_resolved_gateway(resolved, "existing", "existing").await
}

fn configured_gateway_builder(
    session_request: CreateRuntimeSessionRequest,
) -> Result<McpManagementGatewayBuilder, String> {
    let timeout = Duration::from_millis(
        std::env::var("CHATOS_MCP_MANAGEMENT_TOOL_TIMEOUT_MS")
            .ok()
            .and_then(|value| value.parse::<u64>().ok())
            .unwrap_or(DEFAULT_TOOL_TIMEOUT_MS)
            .clamp(1_000, 2 * 60 * 60 * 1_000),
    );
    let ask_user_timeout = Duration::from_millis(
        std::env::var("CHATOS_MCP_MANAGEMENT_ASK_USER_TOOL_TIMEOUT_MS")
            .ok()
            .and_then(|value| value.parse::<u64>().ok())
            .unwrap_or(ASK_USER_TRANSPORT_TIMEOUT_MS)
            .clamp(
                chatos_mcp::ASK_USER_PROMPT_TIMEOUT_MS_DEFAULT,
                7 * 24 * 60 * 60 * 1_000,
            ),
    );
    let mut builder = McpManagementGatewayBuilder::new("chatos", session_request, timeout)
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
    let terminal_wait_timeout =
        Duration::from_millis(chatos_mcp::PROCESS_WAIT_MAX_TIMEOUT_MS + 15_000);
    for tool_name in ["process_wait", "process"] {
        builder = builder.with_tool_timeout(
            format!("{}_{}", terminal_descriptor.server_name, tool_name),
            terminal_wait_timeout,
        );
    }
    Ok(builder)
}

async fn build_resolved_gateway(
    resolved: chatos_mcp_gateway::ResolvedMcpGateway,
    source_session_id: &str,
    turn_id: &str,
) -> Result<McpManagementGateway, String> {
    let mcp_command_queue = resolved.mcp_command_queue.clone();
    let runtime_session = resolved.runtime_session;
    let server = match crate::services::shared_mcp_runtime::chatos_http_server(resolved.server) {
        Ok(server) => server,
        Err(error) => {
            let mcp_session_id = runtime_session.session_id().to_string();
            if let Err(close_error) = runtime_session.close().await {
                warn!(
                    source_session_id,
                    turn_id,
                    mcp_session_id,
                    error = %close_error,
                    "close invalid ChatOS MCP Management runtime session failed"
                );
            }
            return Err(error);
        }
    };
    Ok(McpManagementGateway {
        server,
        effective_mcp_ids: resolved.effective_mcp_ids,
        provider_skills_prompt: resolved.provider_skills_prompt,
        plugin_instruction_items: resolved.plugin_instruction_items,
        mcp_command_queue,
        runtime_session,
    })
}

fn required_text<'a>(value: Option<&'a str>, field: &str) -> Result<&'a str, String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| format!("{field} is required for MCP Management"))
}

fn normalized(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn normalized_unique(values: &[String]) -> Vec<String> {
    let mut values = values
        .iter()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>();
    values.sort();
    values.dedup();
    values
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expected_project_task_ids_are_trimmed_and_deduplicated() {
        assert_eq!(
            normalized_unique(&[
                " task-2 ".to_string(),
                "task-1".to_string(),
                "task-2".to_string(),
                String::new(),
            ]),
            vec!["task-1", "task-2"]
        );
    }
}
