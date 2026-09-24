// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

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
