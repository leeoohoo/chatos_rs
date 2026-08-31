// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashMap;

use chatos_mcp::{system_mcp_descriptor_by_resource_id, SystemMcpKey};
use chatos_mcp_management_sdk::{McpProviderKind, ResolvedMcpRoute};
use chatos_mcp_service::METHOD_TOOLS_LIST;
use chatos_plugin_management_sdk::SystemAgentKey;
use chatos_service_runtime::http_body::read_response_bytes_limited;
use serde_json::{json, Value};

use super::{
    ProviderCallError, TaskRunnerProvider, TaskRunnerRequestBinding, TASK_RUNNER_MCP_LIST_SCOPE,
    TASK_RUNNER_OWNER_SERVICE,
};
use crate::providers::project_service::decode_jsonrpc_response;

impl TaskRunnerProvider {
    #[allow(clippy::too_many_arguments)]
    pub(in crate::providers) async fn prepare_routes(
        &self,
        routes: &mut [ResolvedMcpRoute],
        runtime_session_id: &str,
        owner_user_id: &str,
        agent_key: SystemAgentKey,
        project_id: Option<&str>,
        run_id: Option<&str>,
        turn_id: Option<&str>,
        task_id: Option<&str>,
        source_session_id: Option<&str>,
        source_user_message_id: Option<&str>,
        default_model_config_id: Option<&str>,
        default_remote_connection_id: Option<&str>,
        task_profile: Option<&str>,
        expected_project_task_ids: &[String],
        expires_at_unix: i64,
    ) -> HashMap<String, Vec<Value>> {
        let mut tool_snapshots = HashMap::new();
        let binding = TaskRunnerRequestBinding {
            owner_user_id,
            owner_role: None,
            agent_key: agent_key.as_str(),
            session_id: runtime_session_id,
            expires_at_unix,
            project_id,
            run_id,
            turn_id,
            task_id,
            source_session_id,
            source_user_message_id,
            contact_agent_id: None,
            default_model_config_id,
            default_remote_connection_id,
            task_profile,
            expected_project_task_ids,
        };
        for route in routes
            .iter_mut()
            .filter(|route| is_dynamic_task_runner_route(route))
        {
            match self.list_task_runner_tools(&binding).await {
                Ok(tools) if !tools.is_empty() => {
                    tool_snapshots.insert(route.resource_id.clone(), tools);
                }
                Ok(_) => make_task_runner_route_unavailable(
                    route,
                    "Task Runner Service reported no available tools",
                ),
                Err(error) => make_task_runner_route_unavailable(route, error.message.as_str()),
            }
        }
        tool_snapshots
    }

    async fn list_task_runner_tools(
        &self,
        binding: &TaskRunnerRequestBinding<'_>,
    ) -> Result<Vec<Value>, ProviderCallError> {
        let secret = self.internal_secret.as_deref().ok_or_else(|| {
            ProviderCallError::provider_unavailable(
                "Task Runner Provider internal secret is not configured",
            )
        })?;
        let invocation_id = format!("list-task-runner-{}", binding.session_id);
        let endpoint = format!(
            "{}/internal/mcp-management/mcp/{}",
            self.base_url,
            SystemMcpKey::TaskRunnerService.as_str()
        );
        let response = self
            .bound_request(
                binding,
                endpoint,
                self.request_timeout,
                secret,
                TASK_RUNNER_MCP_LIST_SCOPE,
            )?
            .json(&json!({
                "jsonrpc": "2.0",
                "id": invocation_id,
                "method": METHOD_TOOLS_LIST,
                "params": {}
            }))
            .send()
            .await
            .map_err(|error| {
                ProviderCallError::provider_unavailable(format!(
                    "Task Runner Service tools/list request failed: {error}"
                ))
            })?;
        let status = response.status();
        let bytes = read_response_bytes_limited(response, self.response_limit_bytes)
            .await
            .map_err(|error| {
                ProviderCallError::invalid_response(format!(
                    "Task Runner Service tools/list response could not be read: {error}"
                ))
            })?;
        if !status.is_success() {
            return Err(ProviderCallError::provider_unavailable(format!(
                "Task Runner Service tools/list returned HTTP {}",
                status.as_u16()
            )));
        }
        let result = decode_jsonrpc_response(
            bytes.as_slice(),
            invocation_id.as_str(),
            "Task Runner Service tools/list",
        )?;
        extract_task_runner_tool_snapshot(result).map_err(ProviderCallError::invalid_response)
    }
}

fn is_dynamic_task_runner_route(route: &ResolvedMcpRoute) -> bool {
    route.provider_kind == McpProviderKind::InternalService
        && route.provider_ref.as_deref() == Some(TASK_RUNNER_OWNER_SERVICE)
        && system_mcp_descriptor_by_resource_id(route.resource_id.as_str())
            .is_some_and(|descriptor| descriptor.key == SystemMcpKey::TaskRunnerService)
}

fn make_task_runner_route_unavailable(route: &mut ResolvedMcpRoute, reason: &str) {
    route.provider_kind = McpProviderKind::Unavailable;
    route.provider_ref = None;
    route.allow_writes = false;
    route.cancel_supported = false;
    route.reason = format!("Task Runner Service is unavailable: {reason}");
}

fn extract_task_runner_tool_snapshot(result: Value) -> Result<Vec<Value>, String> {
    let tools = result
        .get("tools")
        .and_then(Value::as_array)
        .cloned()
        .ok_or_else(|| "Task Runner Service tools/list response has no tools array".to_string())?;
    if tools.iter().any(|tool| {
        !tool.is_object()
            || tool
                .get("name")
                .and_then(Value::as_str)
                .map(str::trim)
                .is_none_or(str::is_empty)
    }) {
        return Err(
            "Task Runner Service tools/list response contains an invalid tool definition"
                .to_string(),
        );
    }
    Ok(tools)
}
