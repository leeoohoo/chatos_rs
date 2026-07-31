// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashSet;

use chatos_mcp::{system_mcp_descriptor_for_record, system_mcp_tool_catalog, SystemMcpToolCatalog};
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
        let source_tools = tool_snapshot(resolved)?;
        if source_tools.is_empty() && resolved.binding.required {
            missing_required_tool_schemas.push(resolved.resource.id.clone());
        }
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
    }
    tools.sort_by(|left, right| left.exposed_name.cmp(&right.exposed_name));
    missing_required_tool_schemas.sort();
    missing_required_tool_schemas.dedup();
    Ok(MaterializedRuntimeTools {
        tools,
        missing_required_tool_schemas,
    })
}

pub fn runtime_route_revision(
    base_route_revision: &str,
    tools: &[RuntimeToolDescriptor],
) -> Result<String, String> {
    let bytes = serde_json::to_vec(&(
        "mcp-runtime-route-with-tool-schema-v1",
        base_route_revision,
        tools,
    ))
    .map_err(|err| format!("serialize runtime route snapshot failed: {err}"))?;
    Ok(hex::encode(Sha256::digest(bytes)))
}

fn tool_snapshot(resolved: &ResolvedMcp) -> Result<Vec<Value>, String> {
    let Some(descriptor) = system_mcp_descriptor_for_record(&resolved.resource) else {
        return Ok(resolved.tool_snapshot.clone());
    };
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

    #[test]
    fn external_snapshot_tools_receive_stable_server_namespace() {
        let resolved = ResolvedMcp {
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
        };
        let capabilities = ResolvedAgentCapabilities {
            agent_key: "task_runner_run_phase".to_string(),
            owner_user_id: "user-1".to_string(),
            policy_revision: "policy-1".to_string(),
            generated_at: "now".to_string(),
            agent_enabled: true,
            mcps: vec![resolved],
            skills: Vec::new(),
            plugins: Vec::new(),
            local_connector_requirements: Vec::new(),
        };
        let routes = vec![ResolvedMcpRoute {
            resource_id: "external-1".to_string(),
            server_name: "demo".to_string(),
            provider_kind: McpProviderKind::ExternalHttp,
            provider_ref: Some("mcp-resource:external-1".to_string()),
            tool_namespace: "demo".to_string(),
            allow_writes: false,
            retry_class: McpRetryClass::IdempotentRead,
            cancel_supported: true,
            reason: "test".to_string(),
        }];
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
            runtime_route_revision("base-route", &first).unwrap(),
            runtime_route_revision("base-route", &second).unwrap()
        );
    }
}
