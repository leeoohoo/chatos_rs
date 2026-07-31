// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashSet;

use chatos_mcp::{
    system_mcp_descriptor_by_resource_id, system_mcp_descriptor_for_record,
    system_mcp_tool_catalog, SystemMcpKey, SystemMcpToolCatalog,
};
use chatos_mcp_management_sdk::{ResolvedMcpRoute, RuntimeToolDescriptor};
use chatos_plugin_management_sdk::{ResolvedAgentCapabilities, ResolvedMcp};
use serde_json::Value;
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MaterializedRuntimeTools {
    pub tools: Vec<RuntimeToolDescriptor>,
    pub missing_required_tool_schemas: Vec<String>,
}

pub fn materialize_runtime_tools(
    capabilities: &ResolvedAgentCapabilities,
    routes: &[ResolvedMcpRoute],
) -> Result<MaterializedRuntimeTools, String> {
    let mut seen = HashSet::new();
    let mut tools = Vec::new();
    let mut missing_required_tool_schemas = Vec::new();
    for resolved in capabilities
        .mcps
        .iter()
        .filter(|resolved| resolved.binding.enabled && resolved.resource.enabled)
    {
        let Some(route) = routes
            .iter()
            .find(|route| route.resource_id == resolved.resource.id)
        else {
            continue;
        };
        let source_tools = tool_snapshot(resolved, route)?;
        let exposed_before = tools.len();
        for definition in source_tools {
            let original_name = definition
                .get("name")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| {
                    format!(
                        "MCP {} tool snapshot contains a tool without a name",
                        resolved.resource.id
                    )
                })?;
            if !resource_allows_tool(resolved, route, original_name) {
                continue;
            }
            let exposed_name = route.exposed_tool_name(original_name);
            if !seen.insert(exposed_name.clone()) {
                return Err(format!(
                    "aggregated MCP tool name conflicts: {exposed_name}"
                ));
            }
            let mut exposed_definition = definition.clone();
            let object = exposed_definition.as_object_mut().ok_or_else(|| {
                format!(
                    "MCP {} tool snapshot contains a non-object tool definition",
                    resolved.resource.id
                )
            })?;
            object.insert("name".to_string(), Value::String(exposed_name.clone()));
            tools.push(RuntimeToolDescriptor {
                exposed_name,
                original_name: original_name.to_string(),
                resource_id: resolved.resource.id.clone(),
                definition: exposed_definition,
            });
        }
        if tools.len() == exposed_before && resolved.binding.required {
            missing_required_tool_schemas.push(resolved.resource.id.clone());
        }
    }
    tools.sort_by(|left, right| left.exposed_name.cmp(&right.exposed_name));
    missing_required_tool_schemas.sort();
    missing_required_tool_schemas.dedup();
    Ok(MaterializedRuntimeTools {
        tools,
        missing_required_tool_schemas,
    })
}

fn resource_allows_tool(
    resolved: &ResolvedMcp,
    route: &ResolvedMcpRoute,
    original_tool_name: &str,
) -> bool {
    if route.provider_kind == chatos_mcp_management_sdk::McpProviderKind::Unavailable {
        return false;
    }
    let security = &resolved.resource.security;
    let allowed_by_allowlist = security.allowed_tool_names.is_empty()
        || security
            .allowed_tool_names
            .iter()
            .any(|name| name.trim() == original_tool_name);
    let blocked_by_blocklist = security
        .blocked_tool_names
        .iter()
        .any(|name| name.trim() == original_tool_name);
    allowed_by_allowlist
        && !blocked_by_blocklist
        && route_allows_system_tool(route, original_tool_name)
}

pub fn route_allows_system_tool(route: &ResolvedMcpRoute, original_tool_name: &str) -> bool {
    let Some(descriptor) = system_mcp_descriptor_by_resource_id(route.resource_id.as_str()) else {
        return true;
    };
    if route.allow_writes || !descriptor.allow_writes {
        return true;
    }
    match descriptor.key {
        SystemMcpKey::ProjectManagement => {
            chatos_mcp::project_management_contract::tools::PROJECT_MANAGEMENT_READ_ONLY_TOOL_NAMES
                .contains(&original_tool_name)
        }
        _ => false,
    }
}

pub fn runtime_route_revision(
    base_route_revision: &str,
    policy_revision: &str,
    routes: &[ResolvedMcpRoute],
    tools: &[RuntimeToolDescriptor],
) -> Result<String, String> {
    let bytes = serde_json::to_vec(&(
        "mcp-runtime-route-with-provider-policy-and-tool-schema-v3",
        base_route_revision,
        policy_revision,
        routes,
        tools,
    ))
    .map_err(|err| format!("serialize runtime route snapshot failed: {err}"))?;
    Ok(hex::encode(Sha256::digest(bytes)))
}

fn tool_snapshot(resolved: &ResolvedMcp, route: &ResolvedMcpRoute) -> Result<Vec<Value>, String> {
    let Some(descriptor) = system_mcp_descriptor_for_record(&resolved.resource) else {
        return Ok(resolved.tool_snapshot.clone());
    };
    if descriptor.key == SystemMcpKey::BrowserTools
        && route.provider_kind == chatos_mcp_management_sdk::McpProviderKind::InternalService
        && route.provider_ref.as_deref() == Some("chatos")
    {
        return Ok(resolved.tool_snapshot.clone());
    }
    match system_mcp_tool_catalog(descriptor.key)? {
        SystemMcpToolCatalog::Static(tools) => Ok(tools),
        SystemMcpToolCatalog::Dynamic => Ok(resolved.tool_snapshot.clone()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chatos_mcp_management_sdk::{McpProviderKind, McpRetryClass};
    use chatos_plugin_management_sdk::{
        AgentBindingRecord, BindingConditions, McpRecord, McpRuntime, ResolvedMcp,
        ResourceMetadata, ResourceSecurity,
    };
    use serde_json::json;

    fn resolved_external_mcp() -> ResolvedMcp {
        ResolvedMcp {
            resource: McpRecord {
                id: "external-1".to_string(),
                owner_user_id: "user-1".to_string(),
                owner_kind: "user".to_string(),
                visibility: "private".to_string(),
                source_kind: "user_created".to_string(),
                name: "demo".to_string(),
                display_name: "Demo".to_string(),
                description: None,
                enabled: true,
                runtime: McpRuntime {
                    kind: "http".to_string(),
                    server_name: Some("demo".to_string()),
                    ..McpRuntime::default()
                },
                security: ResourceSecurity::default(),
                metadata: ResourceMetadata::default(),
                plugin_component: Default::default(),
                created_by: "user-1".to_string(),
                updated_by: "user-1".to_string(),
                created_at: "now".to_string(),
                updated_at: "now".to_string(),
            },
            binding: AgentBindingRecord {
                id: "binding-1".to_string(),
                agent_key: "task_runner_run_phase".to_string(),
                binding_scope: "user_override".to_string(),
                owner_user_id: Some("user-1".to_string()),
                resource_kind: "mcp".to_string(),
                resource_id: "external-1".to_string(),
                enabled: true,
                required: true,
                priority: 100,
                conditions: BindingConditions::default(),
                component_allowlist: Vec::new(),
                created_by: "user-1".to_string(),
                updated_by: "user-1".to_string(),
                created_at: "now".to_string(),
                updated_at: "now".to_string(),
            },
            available: false,
            status: "offline".to_string(),
            reason: Some("offline".to_string()),
            tool_snapshot: vec![json!({
                "name": "search",
                "description": "Search",
                "inputSchema": {"type": "object"}
            })],
        }
    }

    fn capabilities_with_mcp(resolved: ResolvedMcp) -> ResolvedAgentCapabilities {
        ResolvedAgentCapabilities {
            agent_key: "task_runner_run_phase".to_string(),
            owner_user_id: "user-1".to_string(),
            policy_revision: "policy-1".to_string(),
            generated_at: "now".to_string(),
            agent_enabled: true,
            mcps: vec![resolved],
            skills: Vec::new(),
            plugins: Vec::new(),
            local_connector_requirements: Vec::new(),
        }
    }

    fn external_route() -> ResolvedMcpRoute {
        ResolvedMcpRoute {
            resource_id: "external-1".to_string(),
            server_name: "demo".to_string(),
            provider_kind: McpProviderKind::ExternalHttp,
            provider_ref: Some("mcp-resource:external-1".to_string()),
            tool_namespace: "demo".to_string(),
            allow_writes: false,
            retry_class: McpRetryClass::IdempotentRead,
            cancel_supported: true,
            reason: "test".to_string(),
        }
    }

    fn resolved_cloud_browser_mcp() -> ResolvedMcp {
        let descriptor = chatos_mcp::system_mcp_descriptor(SystemMcpKey::BrowserTools);
        let mut resolved = resolved_external_mcp();
        resolved.resource.id = descriptor.resource_id.to_string();
        resolved.resource.name = descriptor.server_name.to_string();
        resolved.resource.runtime.kind = "system".to_string();
        resolved.resource.runtime.system_key = Some(descriptor.key.as_str().to_string());
        resolved.resource.runtime.server_name = Some(descriptor.server_name.to_string());
        resolved.binding.resource_id = descriptor.resource_id.to_string();
        resolved.tool_snapshot = vec![json!({
            "name": "browser_navigate",
            "description": "Navigate",
            "inputSchema": {"type": "object"}
        })];
        resolved
    }

    fn cloud_browser_route() -> ResolvedMcpRoute {
        let descriptor = chatos_mcp::system_mcp_descriptor(SystemMcpKey::BrowserTools);
        ResolvedMcpRoute {
            resource_id: descriptor.resource_id.to_string(),
            server_name: descriptor.server_name.to_string(),
            provider_kind: McpProviderKind::InternalService,
            provider_ref: Some("chatos".to_string()),
            tool_namespace: descriptor.server_name.to_string(),
            allow_writes: true,
            retry_class: McpRetryClass::NoRetry,
            cancel_supported: false,
            reason: "test".to_string(),
        }
    }

    #[test]
    fn external_snapshot_tools_receive_stable_server_namespace() {
        let capabilities = capabilities_with_mcp(resolved_external_mcp());
        let routes = vec![external_route()];
        let materialized = materialize_runtime_tools(&capabilities, &routes).unwrap();
        assert_eq!(materialized.tools[0].exposed_name, "demo_search");
        assert_eq!(
            materialized.tools[0]
                .definition
                .get("name")
                .and_then(Value::as_str),
            Some("demo_search")
        );
        assert!(materialized.missing_required_tool_schemas.is_empty());
    }

    #[test]
    fn external_snapshot_tools_honor_allowlist_and_blocklist() {
        let mut resolved = resolved_external_mcp();
        resolved.resource.security.allowed_tool_names =
            vec!["search".to_string(), "delete".to_string()];
        resolved.resource.security.blocked_tool_names = vec!["delete".to_string()];
        resolved.tool_snapshot = vec![
            json!({"name": "search", "inputSchema": {"type": "object"}}),
            json!({"name": "delete", "inputSchema": {"type": "object"}}),
            json!({"name": "unknown", "inputSchema": {"type": "object"}}),
        ];
        let capabilities = capabilities_with_mcp(resolved);
        let materialized = materialize_runtime_tools(&capabilities, &[external_route()]).unwrap();
        assert_eq!(
            materialized
                .tools
                .iter()
                .map(|tool| tool.original_name.as_str())
                .collect::<Vec<_>>(),
            vec!["search"]
        );
    }

    #[test]
    fn cloud_browser_uses_the_live_chatos_tool_snapshot() {
        let capabilities = capabilities_with_mcp(resolved_cloud_browser_mcp());
        let materialized =
            materialize_runtime_tools(&capabilities, &[cloud_browser_route()]).unwrap();
        assert_eq!(materialized.tools.len(), 1);
        assert_eq!(materialized.tools[0].original_name, "browser_navigate");
        assert!(materialized
            .tools
            .iter()
            .all(|tool| tool.original_name != "browser_route_add"));
        assert!(materialized
            .tools
            .iter()
            .all(|tool| tool.original_name != "browser_cdp_command"));
    }

    #[test]
    fn required_mcp_fails_when_policy_filters_every_tool() {
        let mut resolved = resolved_external_mcp();
        resolved.resource.security.blocked_tool_names = vec!["search".to_string()];
        let capabilities = capabilities_with_mcp(resolved);
        let materialized = materialize_runtime_tools(&capabilities, &[external_route()]).unwrap();
        assert!(materialized.tools.is_empty());
        assert_eq!(
            materialized.missing_required_tool_schemas,
            vec!["external-1"]
        );
    }

    #[test]
    fn runtime_route_revision_changes_with_the_tool_schema_snapshot() {
        let first = vec![RuntimeToolDescriptor {
            exposed_name: "demo_search".to_string(),
            original_name: "search".to_string(),
            resource_id: "external-1".to_string(),
            definition: json!({
                "name": "demo_search",
                "inputSchema": {"type": "object"}
            }),
        }];
        let second = vec![RuntimeToolDescriptor {
            definition: json!({
                "name": "demo_search",
                "inputSchema": {
                    "type": "object",
                    "properties": {"query": {"type": "string"}}
                }
            }),
            ..first[0].clone()
        }];
        assert_ne!(
            runtime_route_revision("base-route", "policy-1", &[], &first).unwrap(),
            runtime_route_revision("base-route", "policy-1", &[], &second).unwrap()
        );
    }

    #[test]
    fn runtime_route_revision_binds_capability_policy_changes() {
        assert_ne!(
            runtime_route_revision("base-route", "policy-1", &[], &[]).unwrap(),
            runtime_route_revision("base-route", "policy-2", &[], &[]).unwrap()
        );
    }

    #[test]
    fn runtime_route_revision_binds_the_selected_provider_target() {
        let route = ResolvedMcpRoute {
            resource_id: "builtin_code_maintainer_read".to_string(),
            server_name: "code_maintainer_read".to_string(),
            provider_kind: McpProviderKind::CloudSandbox,
            provider_ref: Some("sandbox:sandbox-1/lease:lease-1".to_string()),
            tool_namespace: "code_maintainer_read".to_string(),
            allow_writes: false,
            retry_class: McpRetryClass::IdempotentRead,
            cancel_supported: true,
            reason: "test".to_string(),
        };
        let mut another = route.clone();
        another.provider_ref = Some("sandbox:sandbox-2/lease:lease-2".to_string());
        assert_ne!(
            runtime_route_revision("base-route", "policy-1", &[route], &[]).unwrap(),
            runtime_route_revision("base-route", "policy-1", &[another], &[]).unwrap()
        );
    }

    #[test]
    fn read_only_project_management_route_blocks_mutating_tools() {
        let route = ResolvedMcpRoute {
            resource_id: "builtin_project_management".to_string(),
            server_name: "project_management_service".to_string(),
            provider_kind: McpProviderKind::InternalService,
            provider_ref: Some("project_management_service".to_string()),
            tool_namespace: "project_management_service".to_string(),
            allow_writes: false,
            retry_class: McpRetryClass::IdempotentRead,
            cancel_supported: true,
            reason: "test".to_string(),
        };
        assert!(route_allows_system_tool(&route, "list_requirements"));
        assert!(!route_allows_system_tool(&route, "create_requirement"));
    }
}
