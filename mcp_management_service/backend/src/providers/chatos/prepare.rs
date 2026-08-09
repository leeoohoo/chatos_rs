// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashMap;

use chatos_mcp::{system_mcp_descriptor_by_resource_id, SystemMcpKey};
use chatos_mcp_management_sdk::{
    McpProviderKind, ResolvedMcpRoute, SandboxExecutionTarget, SandboxProviderKind,
};
use chatos_mcp_service::METHOD_TOOLS_LIST;
use chatos_plugin_management_sdk::SystemAgentKey;
use chatos_service_runtime::http_body::read_response_bytes_limited;
use serde_json::{json, Value};

use super::{ChatosProvider, ChatosRequestBinding, ProviderCallError, CHATOS_PROVIDER_REF};
use crate::providers::project_service::decode_jsonrpc_response;

impl ChatosProvider {
    #[allow(clippy::too_many_arguments)]
    pub(in crate::providers) async fn prepare_routes(
        &self,
        routes: &mut [ResolvedMcpRoute],
        runtime_session_id: &str,
        owner_user_id: &str,
        agent_key: SystemAgentKey,
        project_id: &str,
        run_id: Option<&str>,
        source_session_id: Option<&str>,
        sandbox_target: Option<&SandboxExecutionTarget>,
        expires_at_unix: i64,
    ) -> HashMap<String, Vec<Value>> {
        let mut tool_snapshots = HashMap::new();
        for route in routes
            .iter_mut()
            .filter(|route| is_chatos_browser_route(route))
        {
            let source_session_id = source_session_id
                .map(str::trim)
                .filter(|value| !value.is_empty());
            let Some(source_session_id) = source_session_id else {
                make_browser_route_unavailable(
                    route,
                    "bound source_session_id is required for cloud Browser Tools",
                );
                continue;
            };
            let binding = ChatosRequestBinding {
                owner_user_id,
                agent_key: agent_key.as_str(),
                session_id: runtime_session_id,
                expires_at_unix,
                project_id,
                run_id,
                turn_id: None,
                task_id: None,
                source_session_id: Some(source_session_id),
                source_user_message_id: None,
                default_model_config_id: None,
                contact_agent_id: None,
                expected_project_task_ids: &[],
                sandbox_target,
            };
            match self.list_browser_tools(&binding).await {
                Ok(tools) if !tools.is_empty() => {
                    tool_snapshots.insert(route.resource_id.clone(), tools);
                }
                Ok(_) => make_browser_route_unavailable(
                    route,
                    "ChatOS Browser Runtime reported no available tools",
                ),
                Err(error) => make_browser_route_unavailable(route, error.message.as_str()),
            }
        }
        tool_snapshots
    }

    async fn list_browser_tools(
        &self,
        binding: &ChatosRequestBinding<'_>,
    ) -> Result<Vec<Value>, ProviderCallError> {
        if let Some(target) = binding
            .sandbox_target
            .filter(|target| target.provider == SandboxProviderKind::Cloud)
        {
            self.authorize_sandbox_browser(binding, METHOD_TOOLS_LIST, None)
                .await?;
            let cloud_sandbox = self.cloud_sandbox.as_ref().ok_or_else(|| {
                ProviderCallError::provider_unavailable(
                    "ChatOS Browser Runtime sandbox transport is not configured",
                )
            })?;
            let invocation_id = format!("list-browser-{}", binding.session_id);
            let outcome = cloud_sandbox
                .call_browser_jsonrpc(
                    target,
                    binding.owner_user_id,
                    binding.project_id,
                    binding.run_id,
                    binding.session_id,
                    &json!({
                        "jsonrpc": "2.0",
                        "id": invocation_id,
                        "method": METHOD_TOOLS_LIST,
                        "params": {}
                    }),
                    self.browser_request_timeout,
                )
                .await?;
            let result = decode_jsonrpc_response(
                outcome.body.as_slice(),
                invocation_id.as_str(),
                "Sandbox Browser Runtime tools/list",
            )?;
            return extract_browser_tool_snapshot(result)
                .map_err(ProviderCallError::invalid_response);
        }
        let secret = self.internal_secret.as_deref().ok_or_else(|| {
            ProviderCallError::provider_unavailable(
                "ChatOS Provider internal secret is not configured",
            )
        })?;
        let invocation_id = format!("list-browser-{}", binding.session_id);
        let endpoint = format!(
            "{}/internal/mcp-management/mcp/{}",
            self.base_url,
            SystemMcpKey::BrowserTools.as_str()
        );
        let response = self
            .bound_request(binding, endpoint, self.browser_request_timeout, secret)?
            .json(&json!({
                "jsonrpc": "2.0",
                "id": invocation_id,
                "method": METHOD_TOOLS_LIST,
                "params": {}
            }))
            .send()
            .await
            .map_err(|err| {
                ProviderCallError::provider_unavailable(format!(
                    "ChatOS Browser Runtime tools/list request failed: {err}"
                ))
            })?;
        let status = response.status();
        let bytes = read_response_bytes_limited(response, self.response_limit_bytes)
            .await
            .map_err(|err| {
                ProviderCallError::invalid_response(format!(
                    "ChatOS Browser Runtime tools/list response could not be read: {err}"
                ))
            })?;
        if !status.is_success() {
            return Err(ProviderCallError::provider_unavailable(format!(
                "ChatOS Browser Runtime tools/list returned HTTP {}",
                status.as_u16()
            )));
        }
        let result = decode_jsonrpc_response(
            bytes.as_slice(),
            invocation_id.as_str(),
            "ChatOS Browser Runtime tools/list",
        )?;
        extract_browser_tool_snapshot(result).map_err(ProviderCallError::invalid_response)
    }
}

fn is_chatos_browser_route(route: &ResolvedMcpRoute) -> bool {
    route.provider_kind == McpProviderKind::InternalService
        && route.provider_ref.as_deref() == Some(CHATOS_PROVIDER_REF)
        && system_mcp_descriptor_by_resource_id(route.resource_id.as_str())
            .is_some_and(|descriptor| descriptor.key == SystemMcpKey::BrowserTools)
}

fn make_browser_route_unavailable(route: &mut ResolvedMcpRoute, reason: &str) {
    route.provider_kind = McpProviderKind::Unavailable;
    route.provider_ref = None;
    route.allow_writes = false;
    route.cancel_supported = false;
    route.reason = format!("ChatOS Browser Runtime is unavailable: {reason}");
}

fn extract_browser_tool_snapshot(result: Value) -> Result<Vec<Value>, String> {
    let tools = result
        .get("tools")
        .and_then(Value::as_array)
        .cloned()
        .ok_or_else(|| {
            "ChatOS Browser Runtime tools/list response has no tools array".to_string()
        })?;
    if tools.iter().any(|tool| {
        !tool.is_object()
            || tool
                .get("name")
                .and_then(Value::as_str)
                .map(str::trim)
                .is_none_or(str::is_empty)
    }) {
        return Err(
            "ChatOS Browser Runtime tools/list response contains an invalid tool definition"
                .to_string(),
        );
    }
    Ok(tools)
}
