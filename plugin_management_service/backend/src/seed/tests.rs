// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

#[test]
fn task_runner_run_phase_defaults_cover_execution_capabilities() {
    let kinds = task_runner_run_phase_optional_builtin_kinds()
        .into_iter()
        .map(|(kind, _)| kind)
        .collect::<Vec<_>>();

    assert!(kinds.contains(&BuiltinMcpKind::CodeMaintainerRead));
    assert!(kinds.contains(&BuiltinMcpKind::CodeMaintainerWrite));
    assert!(kinds.contains(&BuiltinMcpKind::TerminalController));
    assert!(kinds.contains(&BuiltinMcpKind::Notepad));
    assert!(kinds.contains(&BuiltinMcpKind::RemoteConnectionController));
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
        let descriptor = chatos_mcp::system_mcp_catalog()
            .iter()
            .find(|descriptor| descriptor.embedded_kind == Some(kind))
            .expect("embedded descriptor");
        let tools = chatos_mcp::system_mcp_static_tools(descriptor.key)
            .unwrap_or_else(|err| panic!("{}: {err}", kind.kind_name()));
        assert!(!tools.is_empty(), "{}", kind.kind_name());
    }
}

#[test]
fn every_system_mcp_has_provider_skills() {
    for descriptor in chatos_mcp::system_mcp_catalog() {
        let skills = provider_skills_for_system_mcp(descriptor.resource_id)
            .and_then(|value| value.as_array().cloned())
            .expect("system MCP provider skills");
        assert!(!skills.is_empty(), "{}", descriptor.resource_id);
        assert!(skills.iter().all(|skill| {
            skill
                .get("instructions")
                .and_then(Value::as_str)
                .is_some_and(|value| !value.trim().is_empty())
        }));
    }
}

#[test]
fn planning_agents_are_retired_without_replacement() {
    assert!(RETIRED_SYSTEM_AGENT_KEYS.contains(&"chatos_plan_agent"));
    assert!(RETIRED_SYSTEM_AGENT_KEYS.contains(&"chatos_planning_agent"));
    assert!(!system_agent_specs()
        .iter()
        .any(|(agent_key, _, _, _, _, _)| *agent_key == "chatos_planning_agent"));
    assert!(RETIRED_SYSTEM_AGENT_KEYS.contains(&"task_runner_plan_phase"));
}

#[test]
fn retired_system_agents_are_unique_and_disjoint_from_the_runtime_catalog() {
    let current = system_agent_specs()
        .into_iter()
        .map(|(agent_key, _, _, _, _, _)| agent_key)
        .collect::<std::collections::HashSet<_>>();
    let retired = RETIRED_SYSTEM_AGENT_KEYS
        .iter()
        .copied()
        .collect::<std::collections::HashSet<_>>();

    assert_eq!(retired.len(), RETIRED_SYSTEM_AGENT_KEYS.len());
    assert!(retired.is_disjoint(&current));
}

#[test]
fn system_agent_registry_contains_all_runtime_roles() {
    let keys = system_agent_specs()
        .into_iter()
        .map(|(agent_key, _, _, _, _, _)| agent_key)
        .collect::<Vec<_>>();

    assert_eq!(
        keys,
        vec![
            CHATOS_CONVERSATION_AGENT_KEY,
            TASK_RUNNER_RUN_AGENT_KEY,
            LOCAL_CONNECTOR_COMMAND_APPROVAL_AGENT_KEY,
            "memory_engine_summary_agent",
            "memory_engine_rollup_agent",
            "memory_engine_subject_memory_agent",
            "memory_engine_memory_rollup_agent",
            "memory_engine_thread_repair_agent",
        ]
    );
}

#[test]
fn memory_generation_agents_are_registered_without_a_tool_plane() {
    let no_tool_plane = system_agent_specs()
        .into_iter()
        .filter(|(_, _, _, _, _, tool_plane)| *tool_plane == AgentToolPlane::None)
        .map(|(agent_key, _, _, _, _, _)| agent_key)
        .collect::<Vec<_>>();

    assert_eq!(
        no_tool_plane,
        vec![
            "memory_engine_summary_agent",
            "memory_engine_rollup_agent",
            "memory_engine_subject_memory_agent",
            "memory_engine_memory_rollup_agent",
            "memory_engine_thread_repair_agent",
        ]
    );
}

#[test]
fn local_command_approval_agent_is_registered_with_a_local_only_tool_plane() {
    let (_, _, _, _, _, tool_plane) = system_agent_specs()
        .into_iter()
        .find(|(agent_key, _, _, _, _, _)| *agent_key == "local_connector_command_approval_agent")
        .expect("local approval agent must be registered");

    assert_eq!(tool_plane, AgentToolPlane::LocalOnly);
    assert!(!tool_plane.uses_managed_gateway());
}

#[test]
fn seeded_system_mcp_records_use_the_unified_runtime_kind() {
    for descriptor in chatos_mcp::system_mcp_catalog() {
        let record = system_mcp_record(descriptor, "admin", "now").expect("system MCP record");
        assert_eq!(record.runtime.kind, RUNTIME_KIND_SYSTEM);
        assert_eq!(
            record.runtime.system_key.as_deref(),
            Some(descriptor.key.as_str())
        );
        assert!(record.runtime.builtin_kind.is_none());
    }
}

#[test]
fn task_process_log_is_a_seeded_task_runner_system_mcp() {
    let descriptor = chatos_mcp::system_mcp_descriptor(
        chatos_plugin_management_sdk::SystemMcpKey::TaskProcessLog,
    );

    assert_eq!(descriptor.resource_id, TASK_PROCESS_LOG_MCP_RESOURCE_ID);
    assert_eq!(descriptor.server_name, "task_run_process");
    let record = system_mcp_record(descriptor, "admin", "now").expect("system MCP record");
    assert_eq!(record.runtime.kind, RUNTIME_KIND_SYSTEM);
    assert_eq!(
        record.runtime.system_key.as_deref(),
        Some(chatos_plugin_management_sdk::SystemMcpKey::TaskProcessLog.as_str())
    );
    let tools = record
        .metadata
        .extra
        .get("tool_catalog")
        .and_then(Value::as_array)
        .expect("tool catalog");
    assert_eq!(
        tools[0].get("name").and_then(Value::as_str),
        Some("record_process")
    );
    assert_eq!(tools.len(), 2);
    assert_eq!(
        tools[1].get("name").and_then(Value::as_str),
        Some("report_outcome")
    );
}
