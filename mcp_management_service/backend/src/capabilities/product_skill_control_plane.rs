// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashSet;

use chatos_mcp::{
    product_skill_runtime_binding, product_skill_runtime_tool_definitions,
    PRODUCT_SKILL_RUNTIME_RESOURCE_ID, PRODUCT_SKILL_RUNTIME_SERVER_NAME,
};
use chatos_mcp_management_sdk::{
    McpProviderKind, McpRetryClass, ResolvedMcpRoute, RuntimeToolDescriptor,
};
use serde_json::Value;

use super::tools::{runtime_product_skill_binding, validate_product_skill_binding};

pub fn append_product_skill_control_plane(
    routes: &mut Vec<ResolvedMcpRoute>,
    tools: &mut Vec<RuntimeToolDescriptor>,
) -> Result<(), String> {
    if !tools.iter().any(|tool| tool.skill_binding.is_some()) {
        return Ok(());
    }
    if routes
        .iter()
        .any(|route| route.resource_id == PRODUCT_SKILL_RUNTIME_RESOURCE_ID)
    {
        return Err(
            "product Skill control-plane route conflicts with an existing route".to_string(),
        );
    }
    let route = ResolvedMcpRoute {
        resource_id: PRODUCT_SKILL_RUNTIME_RESOURCE_ID.to_string(),
        server_name: PRODUCT_SKILL_RUNTIME_SERVER_NAME.to_string(),
        provider_kind: McpProviderKind::InternalService,
        provider_ref: Some("mcp-management:product-skills".to_string()),
        tool_namespace: PRODUCT_SKILL_RUNTIME_SERVER_NAME.to_string(),
        allow_writes: false,
        retry_class: McpRetryClass::IdempotentRead,
        cancel_supported: false,
        reason: "run-bound product Skill resource control plane".to_string(),
    };
    let mut seen = tools
        .iter()
        .map(|tool| tool.exposed_name.clone())
        .collect::<HashSet<_>>();
    let mut control_tools = Vec::new();
    for mut definition in product_skill_runtime_tool_definitions() {
        let original_name = definition
            .get("name")
            .and_then(Value::as_str)
            .ok_or_else(|| "product Skill control tool has no name".to_string())?
            .to_string();
        let exposed_name = route.exposed_tool_name(original_name.as_str());
        if !seen.insert(exposed_name.clone()) {
            return Err(format!(
                "product Skill control tool name conflicts: {exposed_name}"
            ));
        }
        definition
            .as_object_mut()
            .expect("central product Skill tool definitions are objects")
            .insert("name".to_string(), Value::String(exposed_name.clone()));
        let tool = RuntimeToolDescriptor {
            exposed_name,
            original_name: original_name.clone(),
            resource_id: PRODUCT_SKILL_RUNTIME_RESOURCE_ID.to_string(),
            definition,
            skill_binding: product_skill_runtime_binding(original_name.as_str())
                .map(runtime_product_skill_binding),
        };
        validate_product_skill_binding(&route, &tool)?;
        control_tools.push(tool);
    }
    routes.push(route);
    tools.extend(control_tools);
    tools.sort_by(|left, right| left.exposed_name.cmp(&right.exposed_name));
    Ok(())
}
