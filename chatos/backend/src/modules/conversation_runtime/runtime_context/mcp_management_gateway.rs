// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashMap;

use chatos_agent::ChatosAgentProfile;
use chatos_mcp_management_sdk::{
    CreateRuntimeSessionRequest, McpManagementClient, McpManagementClientConfig,
    McpManagementRuntimeSessionHandle, RuntimeSessionResponse,
};
use chatos_plugin_management_sdk::SystemMcpKey;
use tracing::{info, warn};

use crate::services::mcp_loader::McpHttpServer;

const GATEWAY_SERVER_NAME: &str = "mcp_management";
const DEFAULT_TOOL_TIMEOUT_MS: u64 = 180_000;
const ASK_USER_TRANSPORT_TIMEOUT_MS: u64 =
    chatos_mcp::ASK_USER_PROMPT_TIMEOUT_MS_DEFAULT + 5 * 60 * 1_000;

pub(super) struct McpManagementGatewayRequest<'a> {
    pub(super) owner_user_id: Option<&'a str>,
    pub(super) agent_profile: ChatosAgentProfile,
    pub(super) project_id: Option<&'a str>,
    pub(super) source_session_id: Option<&'a str>,
    pub(super) turn_id: Option<&'a str>,
    pub(super) source_user_message_id: Option<&'a str>,
    pub(super) contact_agent_id: Option<&'a str>,
    pub(super) default_model_config_id: Option<&'a str>,
    pub(super) expected_project_task_ids: &'a [String],
    pub(super) locale: Option<&'a str>,
}

pub(super) struct McpManagementGateway {
    server: McpHttpServer,
    effective_mcp_ids: Vec<String>,
    provider_skills_prompt: Option<String>,
    runtime_session: McpManagementRuntimeSessionHandle,
}

impl McpManagementGateway {
    pub(super) fn into_parts(
        self,
    ) -> (
        McpHttpServer,
        Vec<String>,
        Option<String>,
        McpManagementRuntimeSessionHandle,
    ) {
        (
            self.server,
            self.effective_mcp_ids,
            self.provider_skills_prompt,
            self.runtime_session,
        )
    }
}

pub(super) async fn resolve_mcp_management_gateway(
    request: McpManagementGatewayRequest<'_>,
) -> Result<McpManagementGateway, String> {
    let owner_user_id = required_text(request.owner_user_id, "owner_user_id")?;
    let project_id = required_text(request.project_id, "project_id")?;
    let source_session_id = required_text(request.source_session_id, "source_session_id")?;
    let turn_id = required_text(request.turn_id, "turn_id")?;
    let source_user_message_id =
        required_text(request.source_user_message_id, "source_user_message_id")?;
    let config = McpManagementClientConfig::from_env("chatos").await;
    let client = McpManagementClient::new(config)
        .map_err(|err| format!("initialize MCP Management client failed: {err}"))?;
    let session_request = CreateRuntimeSessionRequest {
        owner_user_id: owner_user_id.to_string(),
        agent_key: request.agent_profile.key().as_str().to_string(),
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
        expected_project_task_ids: normalized_unique(request.expected_project_task_ids),
        locale: normalized(request.locale),
        requested_device_id: None,
        requested_sandbox_provider: None,
        sandbox_target: None,
    };
    let session = client
        .resolve_runtime_session(&session_request)
        .await
        .map_err(|err| format!("resolve MCP Management runtime session failed: {err}"))?;
    info!(
        source_session_id,
        turn_id,
        agent_key = request.agent_profile.key().as_str(),
        session_id = session.session_id.as_str(),
        route_revision = session.route_revision.as_str(),
        configured_mcp_count = session.configured_mcp_count,
        exposed_tool_count = session.exposed_tool_count,
        "ChatOS resolved MCP Management runtime session"
    );
    let effective_mcp_ids = session.effective_mcp_ids.clone();
    let provider_skills_prompt = session.provider_skills_prompt.clone();
    let runtime_session =
        McpManagementRuntimeSessionHandle::new(client, session.session_id.clone());
    let server = match gateway_server(session) {
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
        effective_mcp_ids,
        provider_skills_prompt,
        runtime_session,
    })
}

fn gateway_server(session: RuntimeSessionResponse) -> Result<McpHttpServer, String> {
    let timeout_ms = std::env::var("CHATOS_MCP_MANAGEMENT_TOOL_TIMEOUT_MS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(DEFAULT_TOOL_TIMEOUT_MS)
        .clamp(1_000, 2 * 60 * 60 * 1_000);
    let ask_user_timeout_ms = std::env::var("CHATOS_MCP_MANAGEMENT_ASK_USER_TOOL_TIMEOUT_MS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(ASK_USER_TRANSPORT_TIMEOUT_MS)
        .clamp(
            chatos_mcp::ASK_USER_PROMPT_TIMEOUT_MS_DEFAULT,
            7 * 24 * 60 * 60 * 1_000,
        );
    let descriptor = chatos_mcp::system_mcp_descriptor(SystemMcpKey::AskUser);
    let mut tool_timeout_ms = HashMap::new();
    for tool in chatos_mcp::system_mcp_static_tools(SystemMcpKey::AskUser)? {
        let Some(name) = tool.get("name").and_then(serde_json::Value::as_str) else {
            return Err("Ask User MCP tool catalog contains a tool without a name".to_string());
        };
        tool_timeout_ms.insert(
            format!("{}_{}", descriptor.server_name, name.trim()),
            ask_user_timeout_ms,
        );
    }
    let terminal_descriptor = chatos_mcp::system_mcp_descriptor(SystemMcpKey::TerminalController);
    let terminal_wait_timeout_ms = chatos_mcp::PROCESS_WAIT_MAX_TIMEOUT_MS + 15_000;
    for tool_name in ["process_wait", "process"] {
        tool_timeout_ms.insert(
            format!("{}_{}", terminal_descriptor.server_name, tool_name),
            terminal_wait_timeout_ms,
        );
    }
    Ok(McpHttpServer {
        name: GATEWAY_SERVER_NAME.to_string(),
        url: session.mcp_server_url,
        headers: Some(HashMap::from([(
            "authorization".to_string(),
            format!("Bearer {}", session.runtime_token),
        )])),
        timeout_ms: Some(timeout_ms),
        tool_timeout_ms,
        allowed_tool_names: None,
        preserve_tool_names: true,
        fail_on_unavailable: true,
        header_provider: None,
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
    fn gateway_server_uses_runtime_grant_and_preserves_aggregated_names() {
        let server = gateway_server(RuntimeSessionResponse {
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
        })
        .expect("gateway server");
        assert!(server.preserve_tool_names);
        assert!(server.fail_on_unavailable);
        assert_eq!(server.timeout_ms, Some(DEFAULT_TOOL_TIMEOUT_MS));
        assert_eq!(
            server
                .tool_timeout_ms
                .get("ask_user_prompt_choices")
                .copied(),
            Some(ASK_USER_TRANSPORT_TIMEOUT_MS)
        );
        assert_eq!(
            server
                .tool_timeout_ms
                .get("terminal_controller_process_wait")
                .copied(),
            Some(chatos_mcp::PROCESS_WAIT_MAX_TIMEOUT_MS + 15_000)
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
