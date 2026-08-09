// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_mcp_management_sdk::{McpProviderKind, ResolvedMcpRoute};
use chatos_plugin_management_sdk::{PluginComponentKind, PluginManagementClient};
use serde_json::Value;

use super::result::*;
use super::validation::*;
use super::{
    PluginComponentProvider, ProviderCallError, ProviderCallOutcome, AGENT_TOOL_NAME,
    COMMAND_TOOL_NAME,
};
use crate::runtime::{
    PluginCloudToolComponentBinding, PluginToolComponentRuntimeBinding, RuntimeSessionSnapshot,
};

impl PluginComponentProvider {
    pub(super) async fn prepare_cloud(
        &self,
        plugin_management: &PluginManagementClient,
        immutable: &PluginToolComponentRuntimeBinding,
        route: &ResolvedMcpRoute,
    ) -> Result<PluginCloudToolComponentBinding, ProviderCallError> {
        validate_immutable_route(immutable, route, McpProviderKind::PluginCloud)?;
        validate_cloud_component_policy(immutable)?;
        let bundle = plugin_management
            .get_plugin_cloud_component_bundle_for_service(
                immutable.plugin_id.as_str(),
                immutable.release_id.as_str(),
                immutable.component.component_key.as_str(),
            )
            .await
            .map_err(|error| {
                ProviderCallError::provider_unavailable(format!(
                    "load immutable Plugin cloud component Bundle failed: {error}"
                ))
            })?;
        validate_cloud_component_bundle(immutable, &bundle)?;
        let tools = match immutable.component.kind {
            PluginComponentKind::Command => vec![command_tool_definition(immutable)],
            PluginComponentKind::Agent => vec![agent_tool_definition(immutable)],
            _ => unreachable!("validated cloud Plugin component kind"),
        };
        Ok(PluginCloudToolComponentBinding {
            runtime: immutable.clone(),
            bundle,
            tools,
        })
    }

    pub(super) fn call_cloud(
        &self,
        snapshot: &RuntimeSessionSnapshot,
        route: &ResolvedMcpRoute,
        original_tool_name: &str,
        arguments: Value,
    ) -> Result<ProviderCallOutcome, ProviderCallError> {
        let binding = snapshot
            .plugin_cloud_tool_component_bindings
            .get(route.resource_id.as_str())
            .ok_or_else(|| {
                ProviderCallError::provider_unavailable(
                    "Plugin Cloud tool component binding is missing",
                )
            })?;
        validate_cloud_bound_route(snapshot, route, binding)?;
        validate_cloud_component_policy(&binding.runtime)?;
        if !binding.publishes_tool(original_tool_name) {
            return Err(ProviderCallError {
                code: chatos_mcp_service::MCP_ERROR_AUTH_REQUIRED,
                message: "tool is not published by the immutable Plugin cloud component snapshot"
                    .to_string(),
            });
        }
        let result = match binding.runtime.component.kind {
            PluginComponentKind::Command => {
                ensure_expected_tool(original_tool_name, COMMAND_TOOL_NAME)?;
                let arguments = parse_command_arguments(arguments)?;
                plugin_command_result_from_bundle(
                    &binding.runtime,
                    &binding.bundle,
                    arguments.as_deref(),
                )
            }
            PluginComponentKind::Agent => {
                ensure_expected_tool(original_tool_name, AGENT_TOOL_NAME)?;
                validate_empty_arguments(&arguments, "Plugin Agent apply")?;
                plugin_agent_result_from_bundle(&binding.runtime, &binding.bundle)
            }
            _ => Err(ProviderCallError::provider_unavailable(
                "Plugin cloud component kind is not callable",
            )),
        }?;
        let response_bytes = serde_json::to_vec(&result)
            .map_err(|error| ProviderCallError::invalid_response(error.to_string()))?
            .len();
        Ok(ProviderCallOutcome {
            result,
            response_bytes,
        })
    }
}
