// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_plugin_management_sdk::{
    PluginCommandInvocation, PluginComponentKind, ResolvedAgentCapabilities, SelectedPluginRef,
    SystemAgentKey, TaskPluginConfig,
};
use serde_json::json;

use super::super::{TaskRunnerCapabilityPolicy, BUILTIN_RUNTIME_KIND};
use super::fixtures::*;

const RUN_AGENT_KEY: &str = SystemAgentKey::TaskRunnerRunPhase.as_str();
const PLAN_AGENT_KEY: &str = SystemAgentKey::TaskRunnerPlanPhase.as_str();

fn local_runtime_capabilities() -> ResolvedAgentCapabilities {
    policy().capabilities
}

fn local_runtime_policy(
    capabilities: ResolvedAgentCapabilities,
) -> Result<TaskRunnerCapabilityPolicy, String> {
    TaskRunnerCapabilityPolicy::new(capabilities)
}

#[test]
fn plugin_selection_omits_plugin_agent_profiles() {
    let mut capabilities = local_runtime_capabilities();
    capabilities.plugins = vec![resolved_plugin(false)];
    capabilities.mcps.push(resolved_mcp(
        "browser-tools",
        BUILTIN_RUNTIME_KIND,
        Some("BrowserTools"),
        false,
        true,
    ));
    let policy = local_runtime_policy(capabilities).expect("Plugin policy");
    let mut task = task();
    task.plugin_config = TaskPluginConfig {
        selected_plugins: vec![SelectedPluginRef {
            plugin_id: "plugin-browser".to_string(),
            selected_skill_ids: Vec::new(),
            selected_command_ids: Vec::new(),
            selected_agent_ids: vec!["legacy-reviewer".to_string()],
        }],
        command_invocations: Vec::new(),
    };

    policy
        .apply_to_task(&mut task)
        .expect("apply Plugin policy");
    assert!(task.plugin_config.selected_plugins[0]
        .selected_agent_ids
        .is_empty());
    assert!(task
        .mcp_config
        .enabled_builtin_kinds
        .iter()
        .any(|kind| kind == "BrowserTools"));
}

#[test]
fn selected_command_remains_in_task_config_for_mcp_session() {
    let mut capabilities = local_runtime_capabilities();
    capabilities.plugins = vec![resolved_command_plugin(false)];
    let policy = local_runtime_policy(capabilities).expect("Command Plugin policy");
    let mut task = task();
    task.plugin_config = TaskPluginConfig {
        selected_plugins: vec![SelectedPluginRef {
            plugin_id: "plugin-browser".to_string(),
            selected_skill_ids: Vec::new(),
            selected_command_ids: vec!["review".to_string()],
            selected_agent_ids: Vec::new(),
        }],
        command_invocations: vec![PluginCommandInvocation {
            plugin_id: "plugin-browser".to_string(),
            command_id: "review".to_string(),
            arguments: Some("src/lib.rs".to_string()),
        }],
    };

    policy
        .apply_to_task(&mut task)
        .expect("apply Command Plugin policy");
    assert_eq!(task.plugin_config.selected_plugins.len(), 1);
    assert_eq!(
        task.plugin_config.selected_plugins[0].plugin_id,
        "plugin-browser"
    );
    assert_eq!(
        task.plugin_config.selected_plugins[0].selected_command_ids,
        vec!["review"]
    );
    assert_eq!(task.plugin_config.command_invocations.len(), 1);
    assert_eq!(
        task.plugin_config.command_invocations[0]
            .arguments
            .as_deref(),
        Some("src/lib.rs")
    );
}

#[test]
fn plugin_agent_profiles_are_not_task_capabilities() {
    let mut capabilities = local_runtime_capabilities();
    capabilities.plugins = vec![resolved_agent_plugin(RUN_AGENT_KEY)];
    let policy = local_runtime_policy(capabilities).expect("Agent Plugin policy");
    assert!(policy.selectable_plugin_views().is_empty());

    let config = TaskPluginConfig {
        selected_plugins: vec![SelectedPluginRef {
            plugin_id: "plugin-browser".to_string(),
            selected_skill_ids: Vec::new(),
            selected_command_ids: Vec::new(),
            selected_agent_ids: vec!["reviewer".to_string()],
        }],
        command_invocations: Vec::new(),
    };
    assert!(policy
        .validate_plugin_config(&config)
        .expect_err("task-level Plugin Agent selection must fail")
        .contains("not supported"));
}

#[test]
fn plugin_agent_profiles_are_hidden_for_every_task_runner_agent() {
    let mut run_capabilities = local_runtime_capabilities();
    run_capabilities.plugins = vec![resolved_agent_plugin(PLAN_AGENT_KEY)];
    let run_policy = local_runtime_policy(run_capabilities)
        .expect("incompatible optional Agent components are filtered");
    assert!(run_policy.selectable_plugin_views().is_empty());

    let mut plan_capabilities = local_runtime_capabilities();
    plan_capabilities.agent_key = SystemAgentKey::TaskRunnerPlanPhase.as_str().to_string();
    plan_capabilities.plugins = vec![resolved_agent_plugin(PLAN_AGENT_KEY)];
    let policy = local_runtime_policy(plan_capabilities).expect("plan Agent Plugin policy");
    assert!(policy.selectable_plugin_views().is_empty());
}

#[test]
fn a_task_may_not_select_a_plugin_agent() {
    let mut capabilities = local_runtime_capabilities();
    capabilities.plugins = vec![resolved_agent_plugin(RUN_AGENT_KEY)];
    let policy = local_runtime_policy(capabilities).expect("Agent Plugin policy");
    let config = TaskPluginConfig {
        selected_plugins: vec![SelectedPluginRef {
            plugin_id: "plugin-browser".to_string(),
            selected_skill_ids: Vec::new(),
            selected_command_ids: Vec::new(),
            selected_agent_ids: vec!["reviewer".to_string()],
        }],
        ..TaskPluginConfig::default()
    };
    assert!(policy
        .validate_plugin_config(&config)
        .expect_err("Plugin Agent selection must fail")
        .contains("not supported"));
}

#[test]
fn command_requiring_confirmation_is_preserved_for_local_device_approval() {
    let mut capabilities = local_runtime_capabilities();
    capabilities.plugins = vec![resolved_command_plugin(true)];
    let policy = local_runtime_policy(capabilities).expect("Command Plugin policy");
    let config = TaskPluginConfig {
        selected_plugins: vec![SelectedPluginRef {
            plugin_id: "plugin-browser".to_string(),
            selected_skill_ids: Vec::new(),
            selected_command_ids: vec!["review".to_string()],
            selected_agent_ids: Vec::new(),
        }],
        ..TaskPluginConfig::default()
    };

    policy
        .validate_plugin_config(&config)
        .expect("confirmation is enforced by the Local Connector at prepare time");
}

#[test]
fn command_invocation_arguments_must_reference_one_exact_selected_command() {
    let mut capabilities = local_runtime_capabilities();
    capabilities.plugins = vec![resolved_command_plugin(false)];
    let policy = local_runtime_policy(capabilities).expect("Command Plugin policy");
    let mut config = TaskPluginConfig {
        selected_plugins: vec![SelectedPluginRef {
            plugin_id: "plugin-browser".to_string(),
            selected_skill_ids: Vec::new(),
            selected_command_ids: vec!["review".to_string()],
            selected_agent_ids: Vec::new(),
        }],
        command_invocations: vec![PluginCommandInvocation {
            plugin_id: "plugin-browser".to_string(),
            command_id: "unknown".to_string(),
            arguments: Some("src/lib.rs".to_string()),
        }],
    };
    assert!(policy
        .validate_plugin_config(&config)
        .expect_err("unselected Command invocation must fail")
        .contains("unselected Command"));

    config.command_invocations[0].command_id = "review".to_string();
    config
        .command_invocations
        .push(config.command_invocations[0].clone());
    assert!(policy
        .validate_plugin_config(&config)
        .expect_err("duplicate Command invocation must fail")
        .contains("duplicated"));

    config.command_invocations.truncate(1);
    config.command_invocations[0].arguments = Some("x".repeat(16 * 1024 + 1));
    assert!(policy
        .validate_plugin_config(&config)
        .expect_err("oversized Command arguments must fail")
        .contains("exceed"));
}

#[test]
fn command_targeting_the_plan_agent_is_not_selectable_for_run_phase() {
    let mut command_plugin = resolved_command_plugin(false);
    let component = command_plugin
        .components
        .iter_mut()
        .find(|component| component.component.kind == PluginComponentKind::Command)
        .expect("Command component");
    component
        .component
        .metadata
        .insert("target_agent".to_string(), json!(PLAN_AGENT_KEY));
    let mut capabilities = local_runtime_capabilities();
    capabilities.plugins = vec![command_plugin];
    let policy = local_runtime_policy(capabilities).expect("Command Plugin policy");
    let config = TaskPluginConfig {
        selected_plugins: vec![SelectedPluginRef {
            plugin_id: "plugin-browser".to_string(),
            selected_skill_ids: Vec::new(),
            selected_command_ids: vec!["review".to_string()],
            selected_agent_ids: Vec::new(),
        }],
        ..TaskPluginConfig::default()
    };

    let error = policy
        .validate_plugin_config(&config)
        .expect_err("incompatible target Agent must fail");
    assert!(
        error.contains("not selectable") || error.contains("incompatible"),
        "unexpected fail-closed error: {error}"
    );
}

#[test]
fn required_plugin_is_injected_into_effective_task_config() {
    let mut capabilities = local_runtime_capabilities();
    capabilities.plugins = vec![resolved_plugin(true)];
    capabilities.mcps.push(resolved_mcp(
        "browser-tools",
        BUILTIN_RUNTIME_KIND,
        Some("BrowserTools"),
        false,
        true,
    ));
    let policy = local_runtime_policy(capabilities).expect("required Plugin policy");
    let mut task = task();

    policy
        .apply_to_task(&mut task)
        .expect("apply required Plugin");

    assert_eq!(task.plugin_config.selected_plugins.len(), 1);
    assert_eq!(
        task.plugin_config.selected_plugins[0].plugin_id,
        "plugin-browser"
    );
}

#[test]
fn cloud_runtime_does_not_inject_optional_plugins_without_task_selection() {
    let mut capabilities = local_runtime_capabilities();
    let mut plugin = resolved_plugin(false);
    plugin.catalog.name = "ponytail".to_string();
    for component in &mut plugin.components {
        component.component.execution_host =
            chatos_plugin_management_sdk::PluginExecutionHost::Cloud;
    }
    for snapshot in &mut plugin.component_snapshots {
        snapshot.component.execution_host =
            chatos_plugin_management_sdk::PluginExecutionHost::Cloud;
    }
    if let Some(release) = plugin.release.as_mut() {
        for component in &mut release.components {
            component.execution_host = chatos_plugin_management_sdk::PluginExecutionHost::Cloud;
        }
    }
    capabilities.plugins = vec![plugin];
    let policy = TaskRunnerCapabilityPolicy::new(capabilities).expect("cloud Plugin policy");
    let mut task = task();

    policy
        .apply_to_task(&mut task)
        .expect("apply cloud Plugin policy");
    assert!(task.plugin_config.selected_plugins.is_empty());
}
