// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::super::*;
use super::fixtures::*;

#[test]
fn ai_selectable_sets_include_every_configured_optional_mcp_capability() {
    let policy = policy();
    assert_eq!(
        policy.selectable_builtin_kind_names(),
        vec![
            "CodeMaintainerRead".to_string(),
            "CodeMaintainerWrite".to_string()
        ]
    );
    assert_eq!(
        policy.selectable_external_mcp_ids(),
        vec!["external-1".to_string()]
    );
}

#[test]
fn runtime_preserves_agent_selection_and_adds_required_mcps() {
    let mut task = task();
    policy().apply_to_task(&mut task).expect("apply policy");
    assert!(task.mcp_config.enabled);
    assert_eq!(
        task.mcp_config.enabled_builtin_kinds,
        vec![
            "AskUser".to_string(),
            "CodeMaintainerRead".to_string(),
            "CodeMaintainerWrite".to_string()
        ]
    );
    assert_eq!(
        task.mcp_config.external_mcp_config_ids,
        vec!["external-1".to_string()]
    );
    assert!(!task
        .mcp_config
        .external_mcp_config_ids
        .contains(&chatos_plugin_management_sdk::TASK_PROCESS_LOG_MCP_RESOURCE_ID.to_string()));
    assert!(task.mcp_config.selected_skill_ids.is_empty());
    let snapshots = policy().skill_snapshots(&task).expect("skill snapshots");
    assert!(snapshots.is_empty());
}

#[test]
fn task_process_log_is_authorized_by_system_mcp_policy_without_becoming_external_mcp() {
    let policy = policy();
    assert!(policy.task_process_log_mcp_enabled());
    assert!(!policy
        .selectable_external_mcp_ids()
        .contains(&chatos_plugin_management_sdk::TASK_PROCESS_LOG_MCP_RESOURCE_ID.to_string()));
}

#[test]
fn disabled_task_process_log_policy_turns_off_run_scoped_process_mcp() {
    let mut policy = policy();
    let process_log = policy
        .capabilities
        .mcps
        .iter_mut()
        .find(|item| {
            item.resource.id == chatos_plugin_management_sdk::TASK_PROCESS_LOG_MCP_RESOURCE_ID
        })
        .expect("process log capability");
    process_log.available = false;
    process_log.status = "disabled".to_string();
    process_log.reason = Some("disabled by config center".to_string());
    process_log.binding.required = false;
    process_log.binding.enabled = false;

    assert!(!policy.task_process_log_mcp_enabled());
}

#[test]
fn planning_policy_materializes_its_configured_mcp_set() {
    let mut policy = policy();
    policy.capabilities.agent_key = SystemAgentKey::TaskRunnerPlanPhase.as_str().to_string();
    for item in &mut policy.capabilities.mcps {
        item.binding.agent_key = SystemAgentKey::TaskRunnerPlanPhase.as_str().to_string();
        if item.resource.id == "external-1" {
            item.resource.security.allow_writes = Some(true);
        }
        if item.resource.id == "write" {
            item.available = true;
            item.status = "available".to_string();
            item.reason = None;
        }
    }
    let mut task = task();
    task.task_profile = crate::models::TASK_PROFILE_CHATOS_PLAN.to_string();
    task.mcp_config.requires_execution = false;
    task.mcp_config.enabled_builtin_kinds = vec![
        "CodeMaintainerRead".to_string(),
        "CodeMaintainerWrite".to_string(),
    ];

    policy.apply_to_task(&mut task).expect("apply plan policy");

    assert!(task
        .mcp_config
        .enabled_builtin_kinds
        .contains(&"CodeMaintainerRead".to_string()));
    assert!(task
        .mcp_config
        .enabled_builtin_kinds
        .contains(&"AskUser".to_string()));
    assert!(task
        .mcp_config
        .enabled_builtin_kinds
        .contains(&"CodeMaintainerWrite".to_string()));
    assert!(!task
        .mcp_config
        .enabled_builtin_kinds
        .contains(&"TerminalController".to_string()));
    assert_eq!(
        policy.selectable_external_mcp_ids(),
        vec!["external-1".to_string()]
    );
}

#[test]
fn planning_policy_accepts_explicitly_configured_mutating_tools() {
    let mut capabilities = policy().capabilities;
    capabilities.agent_key = SystemAgentKey::TaskRunnerPlanPhase.as_str().to_string();
    let write = capabilities
        .mcps
        .iter_mut()
        .find(|item| item.resource.id == "write")
        .expect("write capability");
    write.binding.agent_key = SystemAgentKey::TaskRunnerPlanPhase.as_str().to_string();
    write.binding.required = true;
    write.available = true;
    write.status = "available".to_string();
    write.reason = None;

    let policy = TaskRunnerCapabilityPolicy::new(capabilities)
        .expect("Plugin Management configuration is authoritative");
    let mut task = task();
    policy.apply_to_task(&mut task).expect("apply policy");
    assert!(task
        .mcp_config
        .enabled_builtin_kinds
        .contains(&"CodeMaintainerWrite".to_string()));
}

#[test]
fn policy_rejects_mutating_mcp_when_required_read_dependency_is_not_bound() {
    let mut capabilities = policy().capabilities;
    capabilities
        .mcps
        .retain(|item| plugin_builtin_kind(item) != Some(BuiltinMcpKind::CodeMaintainerRead));
    let write = capabilities
        .mcps
        .iter_mut()
        .find(|item| plugin_builtin_kind(item) == Some(BuiltinMcpKind::CodeMaintainerWrite))
        .expect("write capability");
    write.available = true;
    write.status = "available".to_string();
    write.reason = None;

    let policy = TaskRunnerCapabilityPolicy::new(capabilities)
        .expect("Plugin Management configuration is authoritative");
    let mut config = TaskMcpConfig::default();
    config.enabled_builtin_kinds = vec!["CodeMaintainerWrite".to_string()];
    assert!(policy.validate_optional_config(&config).is_err());
}

#[test]
fn disabled_task_runner_agent_fails_closed() {
    let mut capabilities = policy().capabilities;
    capabilities.agent_enabled = false;
    let error =
        TaskRunnerCapabilityPolicy::new(capabilities).expect_err("disabled Agent must not execute");
    assert!(error.contains("disabled by Plugin Management"));
}

#[test]
fn write_validation_rejects_removed_builtins_but_accepts_configured_offline_resources() {
    let mut config = TaskMcpConfig {
        enabled_builtin_kinds: vec!["TaskManager".to_string()],
        ..TaskMcpConfig::default()
    };
    assert!(policy().validate_optional_config(&config).is_err());
    config.enabled_builtin_kinds = vec!["CodeMaintainerWrite".to_string()];
    assert!(policy().validate_optional_config(&config).is_ok());
    config.external_mcp_config_ids = vec!["revoked".to_string()];
    assert!(policy().validate_optional_config(&config).is_err());
}

#[test]
fn policy_exposes_cloud_and_local_connector_mcps_exactly_as_configured() {
    let mut local = resolved_mcp("local-user", "local_connector_http", None, false, true);
    local.resource.source_kind = "local_connector_discovered".to_string();
    let cloud = resolved_mcp("cloud-http", "http", None, false, true);
    let policy = TaskRunnerCapabilityPolicy::new(ResolvedAgentCapabilities {
        agent_key: SystemAgentKey::TaskRunnerRunPhase.as_str().to_string(),
        owner_user_id: "owner-1".to_string(),
        policy_revision: "revision-local".to_string(),
        generated_at: "now".to_string(),
        agent_enabled: true,
        mcps: vec![local, cloud],
        skills: Vec::new(),
        plugins: Vec::new(),
        local_connector_requirements: Vec::new(),
    })
    .expect("policy");

    assert_eq!(
        policy.selectable_external_mcp_ids(),
        vec!["local-user".to_string(), "cloud-http".to_string()]
    );
}
