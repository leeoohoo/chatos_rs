// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde_json::Value;

use super::{PluginCloudProvider, ProviderCallError, ProviderCallOutcome, ProviderCancelOutcome};
use crate::runtime::RuntimeSessionSnapshot;
use chatos_mcp_management_sdk::ResolvedMcpRoute;
use chatos_plugin_management_sdk::PluginMcpServer;

impl PluginCloudProvider {
    pub(in crate::providers) async fn call_tool(
        &self,
        snapshot: &RuntimeSessionSnapshot,
        route: &ResolvedMcpRoute,
        original_tool_name: &str,
        arguments: Value,
        invocation_id: &str,
    ) -> Result<ProviderCallOutcome, ProviderCallError> {
        let immutable = snapshot
            .plugin_mcp_bindings
            .get(route.resource_id.as_str())
            .ok_or_else(|| {
                ProviderCallError::provider_unavailable(
                    "immutable Plugin MCP runtime binding is missing",
                )
            })?;
        if !self.supports(route)
            || route.provider_ref.as_deref() != Some(immutable.provider_ref.as_str())
            || route.allow_writes != immutable.allow_writes
        {
            return Err(ProviderCallError::provider_unavailable(
                "Plugin Cloud route does not match its immutable runtime binding",
            ));
        }
        let transport = match &immutable.runtime {
            PluginMcpServer::Stdio { .. } => "stdio",
            PluginMcpServer::Http { .. } => "http",
            PluginMcpServer::ConfigFile { .. } => {
                let has_stdio = snapshot
                    .cloud_stdio_bindings
                    .contains_key(route.resource_id.as_str());
                let has_http = snapshot
                    .external_http_bindings
                    .contains_key(route.resource_id.as_str());
                match (has_stdio, has_http) {
                    (true, false) => "stdio",
                    (false, true) => "http",
                    _ => {
                        return Err(ProviderCallError::provider_unavailable(
                            "Plugin Cloud config-file runtime binding is missing or ambiguous",
                        ))
                    }
                }
            }
        };
        match transport {
            "stdio" => {
                let binding = snapshot
                    .cloud_stdio_bindings
                    .get(route.resource_id.as_str())
                    .ok_or_else(|| {
                        ProviderCallError::provider_unavailable(
                            "Plugin Cloud stdio binding is missing",
                        )
                    })?;
                if binding.provider_ref != immutable.provider_ref
                    || binding.allow_writes != immutable.allow_writes
                {
                    return Err(ProviderCallError::provider_unavailable(
                        "Plugin Cloud stdio binding drifted from its immutable snapshot",
                    ));
                }
                self.cloud_stdio
                    .call_bound_tool(
                        snapshot,
                        route.resource_id.as_str(),
                        binding,
                        original_tool_name,
                        arguments,
                        invocation_id,
                    )
                    .await
            }
            "http" => {
                let binding = snapshot
                    .external_http_bindings
                    .get(route.resource_id.as_str())
                    .ok_or_else(|| {
                        ProviderCallError::provider_unavailable(
                            "Plugin Cloud HTTP binding is missing",
                        )
                    })?;
                if binding.provider_ref != immutable.provider_ref
                    || binding.allow_writes != immutable.allow_writes
                {
                    return Err(ProviderCallError::provider_unavailable(
                        "Plugin Cloud HTTP binding drifted from its immutable snapshot",
                    ));
                }
                self.external_http
                    .call_bound_tool(
                        binding,
                        original_tool_name,
                        arguments,
                        invocation_id,
                        "Plugin Cloud HTTP MCP",
                        snapshot.tool_result_max_chars,
                    )
                    .await
            }
            _ => unreachable!("validated Plugin Cloud transport"),
        }
    }

    pub(in crate::providers) async fn cancel_invocation(
        &self,
        snapshot: &RuntimeSessionSnapshot,
        route: &ResolvedMcpRoute,
        invocation_id: &str,
    ) -> Result<ProviderCancelOutcome, ProviderCallError> {
        let immutable = snapshot
            .plugin_mcp_bindings
            .get(route.resource_id.as_str())
            .ok_or_else(|| {
                ProviderCallError::provider_unavailable(
                    "immutable Plugin MCP runtime binding is missing",
                )
            })?;
        if !self.supports(route)
            || route.provider_ref.as_deref() != Some(immutable.provider_ref.as_str())
            || route.allow_writes != immutable.allow_writes
        {
            return Err(ProviderCallError::provider_unavailable(
                "Plugin Cloud route does not match its immutable runtime binding",
            ));
        }
        let stdio = snapshot
            .cloud_stdio_bindings
            .get(route.resource_id.as_str());
        let http = snapshot
            .external_http_bindings
            .get(route.resource_id.as_str());
        match (stdio, http) {
            (Some(binding), None)
                if binding.provider_ref == immutable.provider_ref
                    && binding.allow_writes == immutable.allow_writes =>
            {
                self.cloud_stdio
                    .cancel_bound_invocation(snapshot, route.resource_id.as_str(), invocation_id)
                    .await
            }
            (None, Some(binding))
                if binding.provider_ref == immutable.provider_ref
                    && binding.allow_writes == immutable.allow_writes =>
            {
                self.external_http
                    .cancel_bound_invocation(binding, invocation_id, "Plugin Cloud HTTP MCP")
                    .await
            }
            _ => Err(ProviderCallError::provider_unavailable(
                "Plugin Cloud runtime binding is missing, ambiguous, or drifted",
            )),
        }
    }
}
