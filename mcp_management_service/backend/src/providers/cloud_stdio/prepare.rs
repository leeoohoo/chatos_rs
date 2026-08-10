// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashMap;

use chatos_mcp_management_sdk::{McpProviderKind, ResolvedMcpRoute, SandboxExecutionTarget};
use chatos_plugin_management_sdk::ResolvedAgentCapabilities;
use serde_json::Value;

use super::{
    prepare_binding, CloudStdioProvider, CloudStdioProviderBinding, RuntimeSessionSnapshot,
};

pub(super) struct CloudStdioRequestContext<'a> {
    pub(super) runtime_session_id: &'a str,
    pub(super) owner_user_id: &'a str,
    pub(super) project_id: &'a str,
    pub(super) run_id: Option<&'a str>,
    pub(super) expires_at_unix: i64,
}

impl<'a> CloudStdioRequestContext<'a> {
    pub(super) fn from_snapshot(snapshot: &'a RuntimeSessionSnapshot) -> Self {
        Self {
            runtime_session_id: snapshot.session_id.as_str(),
            owner_user_id: snapshot.owner_user_id.as_str(),
            project_id: snapshot.project_id.as_str(),
            run_id: snapshot.run_id.as_deref(),
            expires_at_unix: snapshot.expires_at_unix,
        }
    }
}

impl CloudStdioProvider {
    #[allow(clippy::too_many_arguments)]
    pub(in crate::providers) async fn prepare_routes(
        &self,
        capabilities: &ResolvedAgentCapabilities,
        routes: &mut [ResolvedMcpRoute],
        target: Option<&SandboxExecutionTarget>,
        runtime_session_id: &str,
        owner_user_id: &str,
        project_id: &str,
        run_id: Option<&str>,
        expires_at_unix: i64,
    ) -> (
        HashMap<String, CloudStdioProviderBinding>,
        HashMap<String, Vec<Value>>,
    ) {
        let resources = capabilities
            .mcps
            .iter()
            .map(|resolved| (resolved.resource.id.as_str(), resolved))
            .collect::<HashMap<_, _>>();
        let mut bindings = HashMap::new();
        let mut tool_snapshots = HashMap::new();
        for route in routes
            .iter_mut()
            .filter(|route| route.provider_kind == McpProviderKind::CloudStdio)
        {
            let binding = resources
                .get(route.resource_id.as_str())
                .ok_or_else(|| "capability resource is missing".to_string())
                .and_then(|resolved| prepare_binding(resolved, route));
            let binding = match binding {
                Ok(binding) => {
                    route.cancel_supported = true;
                    binding
                }
                Err(reason) => {
                    make_route_unavailable(route, reason.as_str());
                    continue;
                }
            };
            let Some(target) = target else {
                make_route_unavailable(route, "runtime sandbox target is missing");
                continue;
            };
            let context = CloudStdioRequestContext {
                runtime_session_id,
                owner_user_id,
                project_id,
                run_id,
                expires_at_unix,
            };
            match self
                .list_tools(target, &context, route.resource_id.as_str(), &binding)
                .await
            {
                Ok(tools) => {
                    tool_snapshots.insert(route.resource_id.clone(), tools);
                    bindings.insert(route.resource_id.clone(), binding);
                }
                Err(error) => make_route_unavailable(route, error.message.as_str()),
            }
        }
        (bindings, tool_snapshots)
    }
}

pub(super) fn make_route_unavailable(route: &mut ResolvedMcpRoute, reason: &str) {
    route.provider_kind = McpProviderKind::Unavailable;
    route.provider_ref = None;
    route.allow_writes = false;
    route.cancel_supported = false;
    route.reason = format!("Cloud stdio MCP configuration is unavailable: {reason}");
}

pub(super) fn extract_tool_snapshot(result: Value) -> Result<Vec<Value>, String> {
    let tools = result
        .get("tools")
        .and_then(Value::as_array)
        .cloned()
        .ok_or_else(|| "Cloud stdio MCP tools/list response has no tools array".to_string())?;
    for tool in &tools {
        if !tool.is_object()
            || tool
                .get("name")
                .and_then(Value::as_str)
                .map(str::trim)
                .is_none_or(str::is_empty)
        {
            return Err(
                "Cloud stdio MCP tools/list response contains an invalid tool definition"
                    .to_string(),
            );
        }
    }
    Ok(tools)
}
