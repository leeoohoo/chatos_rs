// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::time::Duration;

use chatos_mcp_gateway::McpManagementGatewayBuilder;
use chatos_mcp_management_sdk::{
    CreateRuntimeSessionRequest, McpManagementRuntimeSessionHandle, RuntimeWorkspaceRouteTarget,
};
use chatos_mcp_runtime::McpHttpServer;
use chatos_plugin_management_sdk::{SystemAgentKey, SystemMcpKey};
use tracing::info;

use crate::models::{TaskRecord, TaskRunRecord};

const DEFAULT_TOOL_TIMEOUT_MS: u64 = 2 * 60 * 60 * 1_000;
const ASK_USER_TRANSPORT_TIMEOUT_MS: u64 =
    chatos_mcp::ASK_USER_PROMPT_TIMEOUT_MS_DEFAULT + 5 * 60 * 1_000;
const TERMINAL_WAIT_TRANSPORT_TIMEOUT_MS: u64 = chatos_mcp::PROCESS_WAIT_MAX_TIMEOUT_MS + 15_000;

pub(super) async fn resolve_mcp_management_gateway(
    task: &TaskRecord,
    run: &TaskRunRecord,
    agent_key: SystemAgentKey,
    tool_result_max_chars: usize,
    existing_session_ref: Option<&str>,
    workspace_route: Option<&RuntimeWorkspaceRouteTarget>,
) -> Result<ResolvedMcpManagementGateway, String> {
    let owner_user_id = normalized_task_owner_user_id(task)
        .ok_or_else(|| "task owner user id is required for MCP Management".to_string())?;
    let request = CreateRuntimeSessionRequest {
        tenant_id: task.tenant_id.trim().to_string(),
        owner_user_id,
        owner_role: None,
        agent_key: agent_key.as_str().to_string(),
        project_id: crate::models::normalize_project_id(Some(task.project_id.clone())),
        run_id: Some(run.id.clone()),
        execution_group_id: run
            .workspace_execution
            .as_ref()
            .and_then(|execution| execution.execution_group_id.clone()),
        turn_id: None,
        task_id: Some(task.id.clone()),
        task_profile: Some(task.task_profile.clone()),
        source_session_id: task.source_session_id.clone(),
        source_user_message_id: task.source_user_message_id.clone(),
        contact_agent_id: None,
        default_model_config_id: task.default_model_config_id.clone(),
        tool_result_max_chars: Some(tool_result_max_chars.max(1)),
        expected_project_task_ids: Vec::new(),
        requested_mcp_ids: Some(run.effective_tools.requested_mcp_resource_ids.clone()),
        selected_plugins: task.plugin_config.selected_plugins.clone(),
        plugin_command_invocations: task.plugin_config.command_invocations.clone(),
        locale: Some(if task.mcp_config.locale().is_english() {
            "en-US".to_string()
        } else {
            "zh-CN".to_string()
        }),
        workspace_route: workspace_route.cloned(),
    };
    if run.effective_tools.terminal
        && !matches!(
            workspace_route,
            Some(RuntimeWorkspaceRouteTarget::CloudSandbox { .. })
                | Some(RuntimeWorkspaceRouteTarget::LocalConnector { .. })
        )
    {
        return Err(
            "TerminalController requires a prepared Cloud Sandbox for cloud projects".to_string(),
        );
    }
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
    let resolved = match existing_session_ref {
        Some(session_ref) => builder.resolve_existing(session_ref).await,
        None => builder.resolve().await,
    }
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
        plugin_instruction_items: resolved.plugin_instruction_items,
        mcp_command_queue: resolved.mcp_command_queue,
        runtime_session: resolved.runtime_session,
    })
}

pub(super) struct ResolvedMcpManagementGateway {
    server: McpHttpServer,
    pub(super) provider_skills_prompt: Option<String>,
    pub(super) plugin_instruction_items: Vec<serde_json::Value>,
    mcp_command_queue: String,
    runtime_session: McpManagementRuntimeSessionHandle,
}

impl ResolvedMcpManagementGateway {
    pub(super) fn session_id(&self) -> &str {
        self.runtime_session.session_id()
    }

    pub(super) fn into_parts(self) -> (McpHttpServer, McpManagementRuntimeSessionHandle, String) {
        (self.server, self.runtime_session, self.mcp_command_queue)
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

#[cfg(test)]
mod tests {
    use crate::models::TaskMcpConfig;

    fn requested_mcp_resource_ids(config: &TaskMcpConfig) -> Vec<String> {
        crate::services::workspace_execution::effective_task_tool_snapshot(config)
            .requested_mcp_resource_ids
    }

    #[test]
    fn authoritative_task_mcp_config_maps_to_runtime_resource_scope() {
        let config = TaskMcpConfig {
            enabled_builtin_kinds: vec![
                "BrowserTools".to_string(),
                "CodeMaintainerRead".to_string(),
                "BrowserTools".to_string(),
            ],
            external_mcp_config_ids: vec![
                " external-mcp-1 ".to_string(),
                "external-mcp-1".to_string(),
            ],
            ..TaskMcpConfig::default()
        };

        assert_eq!(
            requested_mcp_resource_ids(&config),
            vec![
                "builtin_browser_tools".to_string(),
                "builtin_code_maintainer_read".to_string(),
                "external-mcp-1".to_string(),
                "system_mcp_task_process_log".to_string(),
            ]
        );
    }

    #[test]
    fn mutating_task_scope_includes_required_read_dependency() {
        let config = TaskMcpConfig {
            enabled_builtin_kinds: vec!["CodeMaintainerWrite".to_string()],
            ..TaskMcpConfig::default()
        };

        assert_eq!(
            requested_mcp_resource_ids(&config),
            vec![
                "builtin_code_maintainer_read".to_string(),
                "builtin_code_maintainer_write".to_string(),
                "system_mcp_task_process_log".to_string(),
            ]
        );
    }

    #[test]
    fn enabled_task_mcp_scope_always_contains_run_process_log() {
        let config = TaskMcpConfig {
            enabled: true,
            ..TaskMcpConfig::default()
        };

        assert!(requested_mcp_resource_ids(&config)
            .contains(&"system_mcp_task_process_log".to_string()));
    }

    #[test]
    fn empty_selection_does_not_programmatically_add_project_tools() {
        let config = TaskMcpConfig {
            requires_execution: false,
            enabled_builtin_kinds: Vec::new(),
            ..TaskMcpConfig::default()
        };

        let ids = requested_mcp_resource_ids(&config);

        assert!(!ids.contains(&"builtin_code_maintainer_read".to_string()));
        assert!(!ids.contains(&"builtin_code_maintainer_write".to_string()));
        assert!(!ids.contains(&"builtin_terminal_controller".to_string()));
    }

    #[test]
    fn execution_flag_does_not_expand_the_selected_tool_scope() {
        let config = TaskMcpConfig {
            requires_execution: true,
            enabled_builtin_kinds: Vec::new(),
            ..TaskMcpConfig::default()
        };

        let ids = requested_mcp_resource_ids(&config);

        assert!(!ids.contains(&"builtin_code_maintainer_read".to_string()));
        assert!(!ids.contains(&"builtin_code_maintainer_write".to_string()));
        assert!(!ids.contains(&"builtin_terminal_controller".to_string()));
    }
}
