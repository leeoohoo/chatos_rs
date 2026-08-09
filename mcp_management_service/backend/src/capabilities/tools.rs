// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::{HashMap, HashSet};

use chatos_mcp::{
    system_mcp_descriptor_by_resource_id, system_mcp_descriptor_for_record,
    system_mcp_tool_catalog, SystemMcpKey, SystemMcpToolCatalog,
};
use chatos_mcp_management_sdk::{ResolvedMcpRoute, RuntimeToolDescriptor};
use chatos_plugin_management_sdk::{AgentBindingRecord, ResolvedAgentCapabilities, ResolvedMcp};
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::runtime::{PluginMcpRuntimeBinding, PluginToolComponentRuntimeBinding};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MaterializedRuntimeTools {
    pub tools: Vec<RuntimeToolDescriptor>,
    pub missing_required_tool_schemas: Vec<String>,
}

pub fn materialize_runtime_tools(
    capabilities: &ResolvedAgentCapabilities,
    routes: &[ResolvedMcpRoute],
) -> Result<MaterializedRuntimeTools, String> {
    materialize_runtime_tools_with_plugins(capabilities, routes, &HashMap::new(), &HashMap::new())
}

pub fn materialize_runtime_tools_with_plugins(
    capabilities: &ResolvedAgentCapabilities,
    routes: &[ResolvedMcpRoute],
    plugin_bindings: &HashMap<String, PluginMcpRuntimeBinding>,
    plugin_tool_snapshots: &HashMap<String, Vec<Value>>,
) -> Result<MaterializedRuntimeTools, String> {
    materialize_runtime_tools_with_plugin_components(
        capabilities,
        routes,
        plugin_bindings,
        plugin_tool_snapshots,
        &HashMap::new(),
        &HashMap::new(),
    )
}

pub fn materialize_runtime_tools_with_plugin_components(
    capabilities: &ResolvedAgentCapabilities,
    routes: &[ResolvedMcpRoute],
    plugin_bindings: &HashMap<String, PluginMcpRuntimeBinding>,
    plugin_tool_snapshots: &HashMap<String, Vec<Value>>,
    plugin_component_bindings: &HashMap<String, PluginToolComponentRuntimeBinding>,
    plugin_component_tool_snapshots: &HashMap<String, Vec<Value>>,
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
    let mut plugin_bindings = plugin_bindings.values().collect::<Vec<_>>();
    plugin_bindings.sort_by(|left, right| left.resource_id.cmp(&right.resource_id));
    for binding in plugin_bindings {
        let route = routes
            .iter()
            .find(|route| route.resource_id == binding.resource_id);
        let exposed_before = tools.len();
        if let (Some(route), Some(source_tools)) = (
            route.filter(|route| route.is_available()),
            plugin_tool_snapshots.get(binding.resource_id.as_str()),
        ) {
            for definition in source_tools {
                let original_name = definition
                    .get("name")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .ok_or_else(|| {
                        format!(
                            "Plugin MCP {} tool snapshot contains a tool without a name",
                            binding.resource_id
                        )
                    })?;
                if !plugin_binding_allows_tool(binding, original_name) {
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
                        "Plugin MCP {} tool snapshot contains a non-object tool definition",
                        binding.resource_id
                    )
                })?;
                object.insert("name".to_string(), Value::String(exposed_name.clone()));
                tools.push(RuntimeToolDescriptor {
                    exposed_name,
                    original_name: original_name.to_string(),
                    resource_id: binding.resource_id.clone(),
                    definition: exposed_definition,
                });
            }
        }
        if tools.len() == exposed_before && binding.required {
            missing_required_tool_schemas.push(binding.resource_id.clone());
        }
    }
    let mut component_bindings = plugin_component_bindings.values().collect::<Vec<_>>();
    component_bindings.sort_by(|left, right| left.resource_id.cmp(&right.resource_id));
    for binding in component_bindings {
        let route = routes
            .iter()
            .find(|route| route.resource_id == binding.resource_id);
        let exposed_before = tools.len();
        if let (Some(route), Some(source_tools)) = (
            route.filter(|route| route.is_available()),
            plugin_component_tool_snapshots.get(binding.resource_id.as_str()),
        ) {
            for definition in source_tools {
                let original_name = definition
                    .get("name")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .ok_or_else(|| {
                        format!(
                            "Plugin component {} tool snapshot contains a tool without a name",
                            binding.resource_id
                        )
                    })?;
                let exposed_name = route.exposed_tool_name(original_name);
                if !seen.insert(exposed_name.clone()) {
                    return Err(format!(
                        "aggregated MCP tool name conflicts: {exposed_name}"
                    ));
                }
                let mut exposed_definition = definition.clone();
                let object = exposed_definition.as_object_mut().ok_or_else(|| {
                    format!(
                        "Plugin component {} tool snapshot contains a non-object tool definition",
                        binding.resource_id
                    )
                })?;
                object.insert("name".to_string(), Value::String(exposed_name.clone()));
                tools.push(RuntimeToolDescriptor {
                    exposed_name,
                    original_name: original_name.to_string(),
                    resource_id: binding.resource_id.clone(),
                    definition: exposed_definition,
                });
            }
        }
        if tools.len() == exposed_before && binding.required {
            missing_required_tool_schemas.push(binding.resource_id.clone());
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

fn plugin_binding_allows_tool(binding: &PluginMcpRuntimeBinding, tool_name: &str) -> bool {
    (binding.tool_allowlist.is_empty()
        || binding
            .tool_allowlist
            .iter()
            .any(|allowed| allowed == tool_name))
        && !binding
            .tool_blocklist
            .iter()
            .any(|blocked| blocked == tool_name)
}

fn binding_allows_tool(binding: &AgentBindingRecord, tool_name: &str) -> bool {
    (binding.tool_allowlist.is_empty()
        || binding
            .tool_allowlist
            .iter()
            .any(|allowed| allowed == tool_name))
        && !binding
            .tool_blocklist
            .iter()
            .any(|blocked| blocked == tool_name)
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
        && binding_allows_tool(&resolved.binding, original_tool_name)
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
        AgentBindingRecord, BindingConditions, McpRecord, McpRuntime, PluginComponentDescriptor,
        PluginComponentKind, PluginExecutionHost, PluginMcpServer, ResolvedMcp, ResourceMetadata,
        ResourceSecurity,
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
                agent_key: chatos_plugin_management_sdk::SystemAgentKey::TaskRunnerRunPhase
                    .as_str()
                    .to_string(),
                binding_scope: "user_override".to_string(),
                owner_user_id: Some("user-1".to_string()),
                resource_kind: "mcp".to_string(),
                resource_id: "external-1".to_string(),
                enabled: true,
                required: true,
                priority: 100,
                conditions: BindingConditions::default(),
                component_allowlist: Vec::new(),
                tool_allowlist: Vec::new(),
                tool_blocklist: Vec::new(),
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
            agent_key: chatos_plugin_management_sdk::SystemAgentKey::TaskRunnerRunPhase
                .as_str()
                .to_string(),
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

    fn plugin_binding() -> PluginMcpRuntimeBinding {
        PluginMcpRuntimeBinding {
            provider_ref: format!("plugin-binding:{}", "b".repeat(64)),
            resource_id: "plugin-mcp-1".to_string(),
            plugin_id: "plugin-1".to_string(),
            release_id: "release-1".to_string(),
            version: "1.0.0".to_string(),
            artifact_sha256: "a".repeat(64),
            normalized_manifest_sha256: "b".repeat(64),
            component_key: "workspace".to_string(),
            component_content_sha256: "c".repeat(64),
            declared_execution_host: PluginExecutionHost::Local,
            installation_device_id: Some("device-1".to_string()),
            permission_snapshot: vec!["workspace.read".to_string()],
            auth_connection_ids: Vec::new(),
            runtime: PluginMcpServer::Http {
                component_key: "workspace".to_string(),
                url: "http://127.0.0.1:4100/mcp".to_string(),
                headers: Default::default(),
                oauth_resource: None,
                connect_timeout_ms: None,
            },
            server_key: None,
            tool_allowlist: vec!["read_file".to_string()],
            tool_blocklist: Vec::new(),
            required: true,
            allow_writes: false,
        }
    }

    fn plugin_route() -> ResolvedMcpRoute {
        ResolvedMcpRoute {
            resource_id: "plugin-mcp-1".to_string(),
            server_name: "plugin_workspace".to_string(),
            provider_kind: McpProviderKind::PluginLocal,
            provider_ref: Some(format!("plugin-binding:{}", "b".repeat(64))),
            tool_namespace: "plugin_workspace".to_string(),
            allow_writes: false,
            retry_class: McpRetryClass::IdempotentRead,
            cancel_supported: false,
            reason: "test".to_string(),
        }
    }

    fn plugin_component_binding() -> PluginToolComponentRuntimeBinding {
        PluginToolComponentRuntimeBinding {
            provider_ref: format!("plugin-tool-binding:{}", "c".repeat(64)),
            resource_id: "plugin-component-review".to_string(),
            plugin_id: "plugin-review".to_string(),
            release_id: "release-review-1".to_string(),
            version: "1.0.0".to_string(),
            artifact_sha256: "a".repeat(64),
            normalized_manifest_sha256: "b".repeat(64),
            component: PluginComponentDescriptor {
                component_key: "review".to_string(),
                kind: PluginComponentKind::Command,
                display_name: "Review".to_string(),
                execution_host: PluginExecutionHost::Cloud,
                runtime_kind: "command".to_string(),
                entrypoint: None,
                required: false,
                permissions: Vec::new(),
                metadata: Default::default(),
            },
            component_content_sha256: "c".repeat(64),
            installation_device_id: None,
            permission_snapshot: Vec::new(),
            auth_connection_ids: Vec::new(),
            required: true,
            allow_writes: false,
        }
    }

    fn plugin_component_route() -> ResolvedMcpRoute {
        ResolvedMcpRoute {
            resource_id: "plugin-component-review".to_string(),
            server_name: "plugin_review_review".to_string(),
            provider_kind: McpProviderKind::PluginCloud,
            provider_ref: Some(format!("plugin-tool-binding:{}", "c".repeat(64))),
            tool_namespace: "plugin_review_review".to_string(),
            allow_writes: false,
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
    fn plugin_provider_snapshot_is_namespaced_and_bound_to_the_component_policy() {
        let binding = plugin_binding();
        let bindings = HashMap::from([(binding.resource_id.clone(), binding)]);
        let snapshots = HashMap::from([(
            "plugin-mcp-1".to_string(),
            vec![
                json!({"name": "read_file", "inputSchema": {"type": "object"}}),
                json!({"name": "write_file", "inputSchema": {"type": "object"}}),
            ],
        )]);
        let materialized = materialize_runtime_tools_with_plugins(
            &capabilities_with_mcp(resolved_external_mcp()),
            &[external_route(), plugin_route()],
            &bindings,
            &snapshots,
        )
        .unwrap();
        assert!(materialized
            .tools
            .iter()
            .any(|tool| tool.exposed_name == "plugin_workspace_read_file"));
        assert!(!materialized
            .tools
            .iter()
            .any(|tool| tool.original_name == "write_file"));
        assert!(materialized.missing_required_tool_schemas.is_empty());
    }

    #[test]
    fn plugin_component_tool_snapshot_is_namespaced_and_required() {
        let binding = plugin_component_binding();
        let bindings = HashMap::from([(binding.resource_id.clone(), binding)]);
        let snapshots = HashMap::from([(
            "plugin-component-review".to_string(),
            vec![json!({
                "name": "invoke",
                "description": "Review",
                "inputSchema": {"type": "object"}
            })],
        )]);
        let materialized = materialize_runtime_tools_with_plugin_components(
            &capabilities_with_mcp(resolved_external_mcp()),
            &[external_route(), plugin_component_route()],
            &HashMap::new(),
            &HashMap::new(),
            &bindings,
            &snapshots,
        )
        .unwrap();
        assert!(materialized
            .tools
            .iter()
            .any(|tool| tool.exposed_name == "plugin_review_review_invoke"));
        assert!(materialized.missing_required_tool_schemas.is_empty());

        let unavailable = materialize_runtime_tools_with_plugin_components(
            &capabilities_with_mcp(resolved_external_mcp()),
            &[external_route()],
            &HashMap::new(),
            &HashMap::new(),
            &bindings,
            &snapshots,
        )
        .unwrap();
        assert_eq!(
            unavailable.missing_required_tool_schemas,
            vec!["plugin-component-review"]
        );
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

#[cfg(test)]
mod agent_policy_tests;
