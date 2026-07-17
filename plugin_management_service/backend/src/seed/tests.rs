// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

#[test]
fn task_runner_run_phase_defaults_match_callable_task_runner_providers() {
    let kinds = task_runner_run_phase_optional_builtin_kinds()
        .into_iter()
        .map(|(kind, _)| kind)
        .collect::<Vec<_>>();

    assert!(kinds.contains(&BuiltinMcpKind::CodeMaintainerRead));
    assert!(kinds.contains(&BuiltinMcpKind::CodeMaintainerWrite));
    assert!(kinds.contains(&BuiltinMcpKind::TerminalController));
    assert!(kinds.contains(&BuiltinMcpKind::ProjectManagement));
    assert!(kinds.contains(&BuiltinMcpKind::Notepad));
    assert!(kinds.contains(&BuiltinMcpKind::RemoteConnectionController));
    assert!(kinds.contains(&BuiltinMcpKind::WebTools));
    assert!(kinds.contains(&BuiltinMcpKind::BrowserTools));
    assert!(!kinds.contains(&BuiltinMcpKind::AgentBuilder));
    assert!(!kinds.contains(&BuiltinMcpKind::MemorySkillReader));
}

#[test]
fn every_seeded_builtin_mcp_has_provider_skills_in_both_locales() {
    for kind in builtin_kinds() {
        let skills = provider_skills_for_builtin_mcp(kind);
        let skills = skills.as_array().expect("provider skills array");
        assert_eq!(skills.len(), 2, "{}", kind.kind_name());
        assert!(skills.iter().all(|skill| {
            skill
                .get("instructions")
                .and_then(Value::as_str)
                .is_some_and(|value| !value.trim().is_empty())
        }));
        assert!(skills
            .iter()
            .any(|skill| { skill.get("locale").and_then(Value::as_str) == Some("zh-CN") }));
        assert!(skills
            .iter()
            .any(|skill| { skill.get("locale").and_then(Value::as_str) == Some("en-US") }));
    }
}

#[test]
fn every_seeded_builtin_mcp_has_a_real_tool_catalog() {
    for kind in builtin_kinds() {
        let tools = chatos_builtin_tools::builtin_tool_catalog(kind)
            .unwrap_or_else(|err| panic!("{}: {err}", kind.kind_name()));
        assert!(!tools.is_empty(), "{}", kind.kind_name());
    }
}

#[test]
fn every_system_routed_mcp_has_provider_skills() {
    for resource_id in [
        SANDBOX_IMAGES_MCP_RESOURCE_ID,
        PROJECT_ENVIRONMENT_MCP_RESOURCE_ID,
        PROJECT_RUNTIME_ENVIRONMENT_MCP_RESOURCE_ID,
        LOCAL_CONNECTOR_APPROVAL_MCP_RESOURCE_ID,
        CHATOS_TASK_RUNNER_MCP_RESOURCE_ID,
    ] {
        let skills = provider_skills_for_system_mcp(resource_id)
            .and_then(|value| value.as_array().cloned())
            .expect("system MCP provider skills");
        assert!(!skills.is_empty(), "{resource_id}");
        assert!(skills.iter().all(|skill| {
            skill
                .get("instructions")
                .and_then(Value::as_str)
                .is_some_and(|value| !value.trim().is_empty())
        }));
    }
}

#[test]
fn legacy_chatos_plan_key_is_replaced_by_the_explicit_planning_role() {
    assert!(RETIRED_SYSTEM_AGENT_KEYS.contains(&"chatos_plan_agent"));
    assert!(system_agent_specs()
        .iter()
        .any(|(agent_key, _, _, _, _)| *agent_key == "chatos_planning_agent"));
}

#[test]
fn system_agent_registry_contains_all_runtime_roles() {
    let keys = system_agent_specs()
        .into_iter()
        .map(|(agent_key, _, _, _, _)| agent_key)
        .collect::<Vec<_>>();

    assert_eq!(
        keys,
        vec![
            "chatos_conversation_agent",
            "chatos_planning_agent",
            "project_requirement_execution_planner_agent",
            "task_runner_run_phase",
            "project_management_agent",
            "local_connector_command_approval_agent",
            "memory_engine_summary_agent",
            "memory_engine_rollup_agent",
            "memory_engine_subject_memory_agent",
            "memory_engine_memory_rollup_agent",
            "memory_engine_thread_repair_agent",
        ]
    );
}

#[test]
fn chatos_uses_the_task_runner_service_mcp_entry() {
    assert_eq!(CHATOS_TASK_RUNNER_MCP_SERVER_NAME, "task_runner_service");
}

#[test]
fn chatos_conversation_requires_task_runner_service() {
    let spec = (
        "chatos_conversation_agent",
        CHATOS_TASK_RUNNER_MCP_RESOURCE_ID,
        true,
    );
    assert_eq!(spec.0, "chatos_conversation_agent");
    assert_eq!(spec.1, "system_mcp_chatos_task_runner");
    assert!(spec.2);
}
