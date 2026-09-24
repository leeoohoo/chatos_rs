// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;
use crate::capabilities::append_product_skill_control_plane;

fn system_route(key: SystemMcpKey) -> ResolvedMcpRoute {
    let descriptor = chatos_mcp::system_mcp_descriptor(key);
    ResolvedMcpRoute {
        resource_id: descriptor.resource_id.to_string(),
        server_name: descriptor.server_name.to_string(),
        provider_kind: McpProviderKind::LocalConnector,
        provider_ref: Some(format!("system:{}", key.as_str())),
        tool_namespace: descriptor.server_name.to_string(),
        allow_writes: descriptor.allow_writes,
        retry_class: McpRetryClass::NoRetry,
        cancel_supported: true,
        reason: "test".to_string(),
    }
}

#[test]
fn system_routes_use_the_central_product_skill_catalog() {
    let terminal = system_route(SystemMcpKey::TerminalController);
    let command = product_skill_binding_for_route(&terminal, "execute_command")
        .expect("terminal command binding");
    assert_eq!(command.binding_id, "terminal.command-execution");
    assert_eq!(
        command.required_skills,
        ["chatos-terminal", "chatos-terminal-command-execution"]
    );

    let observation = product_skill_binding_for_route(&terminal, "process_wait")
        .expect("terminal observation binding");
    assert_eq!(observation.binding_id, "terminal.process-observation");

    let survey = system_route(SystemMcpKey::RequirementSurveyRead);
    let survey_read = product_skill_binding_for_route(&survey, "requirement_survey_get")
        .expect("requirement survey binding");
    assert_eq!(survey_read.primary_skill, "requirement-survey-read-results");

    assert!(product_skill_binding_for_route(&terminal, "future_unreviewed_tool").is_none());
    let unreviewed = RuntimeToolDescriptor {
        exposed_name: "terminal_controller_future_unreviewed_tool".to_string(),
        original_name: "future_unreviewed_tool".to_string(),
        resource_id: terminal.resource_id.clone(),
        definition: json!({"name": "terminal_controller_future_unreviewed_tool"}),
        skill_binding: None,
    };
    assert!(validate_product_skill_binding(&terminal, &unreviewed)
        .is_err_and(|error| error.contains("has no registered product Skill binding")));
}

#[test]
fn dynamic_system_tool_materialization_fails_closed_without_a_binding() {
    let descriptor = chatos_mcp::system_mcp_descriptor(SystemMcpKey::TaskRunnerService);
    let mut resolved = resolved_external_mcp();
    resolved.resource.id = descriptor.resource_id.to_string();
    resolved.resource.name = descriptor.server_name.to_string();
    resolved.resource.runtime.kind = chatos_mcp::SYSTEM_MCP_RUNTIME_KIND.to_string();
    resolved.resource.runtime.system_key = Some(descriptor.key.as_str().to_string());
    resolved.resource.runtime.server_name = Some(descriptor.server_name.to_string());
    resolved.tool_snapshot = vec![json!({
        "name": "future_unreviewed_tool",
        "inputSchema": {"type": "object"}
    })];
    let route = ResolvedMcpRoute {
        resource_id: descriptor.resource_id.to_string(),
        server_name: descriptor.server_name.to_string(),
        provider_kind: McpProviderKind::InternalService,
        provider_ref: Some("task-runner-service".to_string()),
        tool_namespace: descriptor.server_name.to_string(),
        allow_writes: true,
        retry_class: McpRetryClass::NoRetry,
        cancel_supported: true,
        reason: "test".to_string(),
    };

    let error = materialize_runtime_tools(&capabilities_with_mcp(resolved), &[route])
        .expect_err("unreviewed system tool must fail closed");
    assert!(error.contains("has no registered product Skill binding"));
}

#[test]
fn product_skill_control_plane_is_added_only_for_bound_product_tools() {
    let terminal = system_route(SystemMcpKey::TerminalController);
    let mut routes = vec![terminal.clone()];
    let mut tools = vec![RuntimeToolDescriptor {
        exposed_name: "terminal_controller_execute_command".to_string(),
        original_name: "execute_command".to_string(),
        resource_id: terminal.resource_id.clone(),
        definition: json!({"name": "terminal_controller_execute_command"}),
        skill_binding: product_skill_binding_for_route(&terminal, "execute_command"),
    }];

    append_product_skill_control_plane(&mut routes, &mut tools).expect("control plane");

    assert!(routes
        .iter()
        .any(|route| route.resource_id == PRODUCT_SKILL_RUNTIME_RESOURCE_ID));
    let control_tools = tools
        .iter()
        .filter(|tool| tool.resource_id == PRODUCT_SKILL_RUNTIME_RESOURCE_ID)
        .collect::<Vec<_>>();
    assert_eq!(control_tools.len(), 2);
    assert!(control_tools.iter().all(|tool| {
        tool.skill_binding
            .as_ref()
            .is_some_and(|binding| binding.primary_skill == "chatos-skill-runtime")
    }));

    let mut routes = Vec::new();
    let mut unbound_tools = vec![RuntimeToolDescriptor {
        exposed_name: "external_search".to_string(),
        original_name: "search".to_string(),
        resource_id: "external".to_string(),
        definition: json!({"name": "external_search"}),
        skill_binding: None,
    }];
    append_product_skill_control_plane(&mut routes, &mut unbound_tools).expect("no-op");
    assert!(routes.is_empty());
    assert_eq!(unbound_tools.len(), 1);
}

#[test]
fn product_skill_control_plane_rejects_route_and_tool_name_conflicts() {
    let terminal = system_route(SystemMcpKey::TerminalController);
    let binding = product_skill_binding_for_route(&terminal, "execute_command");
    let mut routes = vec![ResolvedMcpRoute {
        resource_id: PRODUCT_SKILL_RUNTIME_RESOURCE_ID.to_string(),
        ..terminal.clone()
    }];
    let mut tools = vec![RuntimeToolDescriptor {
        exposed_name: "terminal_controller_execute_command".to_string(),
        original_name: "execute_command".to_string(),
        resource_id: terminal.resource_id.clone(),
        definition: json!({"name": "terminal_controller_execute_command"}),
        skill_binding: binding.clone(),
    }];
    assert!(append_product_skill_control_plane(&mut routes, &mut tools).is_err());

    let mut routes = vec![terminal];
    tools.push(RuntimeToolDescriptor {
        exposed_name: "product_skill_list_resources".to_string(),
        original_name: "search".to_string(),
        resource_id: "external".to_string(),
        definition: json!({"name": "product_skill_list_resources"}),
        skill_binding: None,
    });
    assert!(append_product_skill_control_plane(&mut routes, &mut tools).is_err());
}
