// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_mcp::system_mcp_descriptor_for_record;
use chatos_mcp_management_sdk::{McpExecutionHost, McpRouteCandidate, McpRouteResourceKind};
use chatos_plugin_management_sdk::{McpRecord, PluginExecutionHost, ResolvedAgentCapabilities};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MaterializedAgentMcps {
    pub policy_revision: String,
    pub resources: Vec<McpRouteCandidate>,
}

pub fn materialize_mcp_candidates(
    capabilities: &ResolvedAgentCapabilities,
) -> MaterializedAgentMcps {
    let resources = capabilities
        .mcps
        .iter()
        .filter(|resolved| resolved.binding.enabled && resolved.resource.enabled)
        .map(|resolved| {
            let resource = &resolved.resource;
            let system_descriptor = system_mcp_descriptor_for_record(resource);
            let (resource_kind, execution_host) = if system_descriptor.is_some() {
                (McpRouteResourceKind::System, None)
            } else if resource.plugin_component.is_release_managed() {
                (
                    McpRouteResourceKind::Plugin,
                    plugin_execution_host(capabilities, resource),
                )
            } else {
                classify_runtime(resource.runtime.kind.as_str())
            };
            let server_name = system_descriptor
                .map(|descriptor| descriptor.server_name.to_string())
                .or_else(|| normalized(resource.runtime.server_name.as_deref()))
                .unwrap_or_else(|| resource.name.clone());
            let allow_writes = resource
                .security
                .allow_writes
                .unwrap_or_else(|| system_descriptor.is_some_and(|item| item.allow_writes));
            McpRouteCandidate {
                resource_id: resource.id.clone(),
                server_name,
                resource_kind,
                system_key: system_descriptor.map(|descriptor| descriptor.key.as_str().to_string()),
                execution_host,
                provider_ref: Some(format!("mcp-resource:{}", resource.id)),
                required: resolved.binding.required,
                allow_writes,
            }
        })
        .collect();
    MaterializedAgentMcps {
        policy_revision: capabilities.policy_revision.clone(),
        resources,
    }
}

fn classify_runtime(kind: &str) -> (McpRouteResourceKind, Option<McpExecutionHost>) {
    match kind.trim() {
        "http" => (
            McpRouteResourceKind::ExternalHttp,
            Some(McpExecutionHost::Cloud),
        ),
        "stdio_cloud" => (McpRouteResourceKind::Stdio, Some(McpExecutionHost::Cloud)),
        "local_connector_stdio" | "local_connector_http" | "local_connector_builtin_proxy" => (
            McpRouteResourceKind::LocalConnector,
            Some(McpExecutionHost::Local),
        ),
        _ => (McpRouteResourceKind::Unsupported, None),
    }
}

fn plugin_execution_host(
    capabilities: &ResolvedAgentCapabilities,
    resource: &McpRecord,
) -> Option<McpExecutionHost> {
    let (plugin_id, release_id, component_key) = resource.plugin_component.complete_identity()?;
    capabilities
        .plugins
        .iter()
        .flat_map(|plugin| plugin.component_snapshots.iter())
        .find(|snapshot| {
            snapshot.plugin_id == plugin_id
                && snapshot.release_id == release_id
                && snapshot.component.component_key == component_key
        })
        .map(|snapshot| match snapshot.component.execution_host {
            PluginExecutionHost::Cloud => McpExecutionHost::Cloud,
            PluginExecutionHost::Local => McpExecutionHost::Local,
            PluginExecutionHost::Portable => McpExecutionHost::Portable,
        })
}

fn normalized(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chatos_plugin_management_sdk::{
        AgentBindingRecord, BindingConditions, McpRuntime, PluginComponentOwnership, ResolvedMcp,
        ResourceMetadata, ResourceSecurity,
    };

    fn capabilities(mcps: Vec<ResolvedMcp>) -> ResolvedAgentCapabilities {
        ResolvedAgentCapabilities {
            agent_key: "task_runner_run_phase".to_string(),
            owner_user_id: "user-1".to_string(),
            policy_revision: "policy-1".to_string(),
            generated_at: "now".to_string(),
            agent_enabled: true,
            mcps,
            skills: Vec::new(),
            plugins: Vec::new(),
            local_connector_requirements: Vec::new(),
        }
    }

    fn resolved_mcp(
        id: &str,
        runtime_kind: &str,
        binding_enabled: bool,
        resource_enabled: bool,
        available: bool,
    ) -> ResolvedMcp {
        ResolvedMcp {
            resource: McpRecord {
                id: id.to_string(),
                owner_user_id: "user-1".to_string(),
                owner_kind: "user".to_string(),
                visibility: "private".to_string(),
                source_kind: "user_created".to_string(),
                name: id.to_string(),
                display_name: id.to_string(),
                description: None,
                enabled: resource_enabled,
                runtime: McpRuntime {
                    kind: runtime_kind.to_string(),
                    server_name: Some(id.to_string()),
                    ..McpRuntime::default()
                },
                security: ResourceSecurity::default(),
                metadata: ResourceMetadata::default(),
                plugin_component: PluginComponentOwnership::default(),
                created_by: "user-1".to_string(),
                updated_by: "user-1".to_string(),
                created_at: "now".to_string(),
                updated_at: "now".to_string(),
            },
            binding: AgentBindingRecord {
                id: format!("binding-{id}"),
                agent_key: "task_runner_run_phase".to_string(),
                binding_scope: "user_override".to_string(),
                owner_user_id: Some("user-1".to_string()),
                resource_kind: "mcp".to_string(),
                resource_id: id.to_string(),
                enabled: binding_enabled,
                required: false,
                priority: 100,
                conditions: BindingConditions::default(),
                component_allowlist: Vec::new(),
                created_by: "user-1".to_string(),
                updated_by: "user-1".to_string(),
                created_at: "now".to_string(),
                updated_at: "now".to_string(),
            },
            available,
            status: if available { "ready" } else { "offline" }.to_string(),
            reason: None,
        }
    }

    #[test]
    fn configured_resource_is_materialized_even_when_health_is_unavailable() {
        let result = materialize_mcp_candidates(&capabilities(vec![resolved_mcp(
            "user-http-mcp",
            "http",
            true,
            true,
            false,
        )]));
        assert_eq!(result.resources.len(), 1);
        assert_eq!(
            result.resources[0].resource_kind,
            McpRouteResourceKind::ExternalHttp
        );
    }

    #[test]
    fn disabled_binding_or_resource_is_not_materialized() {
        let result = materialize_mcp_candidates(&capabilities(vec![
            resolved_mcp("binding-disabled", "http", false, true, true),
            resolved_mcp("resource-disabled", "http", true, false, true),
        ]));
        assert!(result.resources.is_empty());
    }

    #[test]
    fn local_connector_runtime_is_explicitly_pinned_local() {
        let result = materialize_mcp_candidates(&capabilities(vec![resolved_mcp(
            "local-mcp",
            "local_connector_stdio",
            true,
            true,
            true,
        )]));
        assert_eq!(
            result.resources[0].resource_kind,
            McpRouteResourceKind::LocalConnector
        );
        assert_eq!(
            result.resources[0].execution_host,
            Some(McpExecutionHost::Local)
        );
    }
}
