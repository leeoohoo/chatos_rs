// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_plugin_management_sdk::{
    PluginCommandInvocation, PluginComponentKind, ResolvedAgentCapabilities, SelectedPluginRef,
    SystemAgentKey, TaskPluginConfig,
};
use serde_json::json;

use super::super::{TaskRunnerCapabilityPolicy, BUILTIN_RUNTIME_KIND};
use super::fixtures::*;

fn local_runtime_capabilities() -> ResolvedAgentCapabilities {
    policy().capabilities
}

fn local_runtime_policy(
    capabilities: ResolvedAgentCapabilities,
) -> Result<TaskRunnerCapabilityPolicy, String> {
    TaskRunnerCapabilityPolicy::new_for_runtime(capabilities, true)
}

#[test]
fn plugin_selection_requires_exact_device_and_produces_immutable_run_snapshot() {
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
        device_id: Some("device-1".to_string()),
        workspace_id: Some("workspace-1".to_string()),
        selected_plugins: vec![SelectedPluginRef {
            plugin_id: "plugin-browser".to_string(),
            selected_skill_ids: Vec::new(),
            selected_command_ids: Vec::new(),
            selected_agent_ids: Vec::new(),
        }],
        command_invocations: Vec::new(),
    };

    policy
        .apply_to_task(&mut task)
        .expect("apply Plugin policy");
    let snapshots = policy.plugin_snapshots(&task).expect("Plugin snapshots");

    assert_eq!(snapshots.len(), 1);
    assert_eq!(snapshots[0].plugin_id, "plugin-browser");
    assert_eq!(snapshots[0].release_id, "release-browser-1");
    assert_eq!(snapshots[0].device_id.as_deref(), Some("device-1"));
    assert_eq!(snapshots[0].workspace_id.as_deref(), Some("workspace-1"));
    assert_eq!(snapshots[0].artifact_sha256, "a".repeat(64));
    assert_eq!(snapshots[0].component_snapshots.len(), 1);
    assert_eq!(snapshots[0].component_snapshots[0].component_key, "browser");
    assert_eq!(
        snapshots[0].component_snapshots[0].content_sha256,
        "c".repeat(64)
    );
    assert_eq!(snapshots[0].permission_snapshot, vec!["browser.control"]);
    assert_eq!(
        snapshots[0].auth_connection_ids,
        vec!["oauth-browser-account"]
    );
    assert!(task
        .mcp_config
        .enabled_builtin_kinds
        .iter()
        .any(|kind| kind == "BrowserTools"));
}

#[test]
fn plugin_ui_is_pinned_as_a_signed_run_component_without_executable_operations() {
    let mut capabilities = local_runtime_capabilities();
    capabilities.plugins = vec![resolved_ui_plugin()];
    capabilities.mcps.push(resolved_mcp(
        "browser-tools",
        BUILTIN_RUNTIME_KIND,
        Some("BrowserTools"),
        false,
        true,
    ));
    let policy = local_runtime_policy(capabilities).expect("Plugin UI policy");
    let mut task = task();
    task.plugin_config = TaskPluginConfig {
        device_id: Some("device-1".to_string()),
        workspace_id: Some("workspace-1".to_string()),
        selected_plugins: vec![SelectedPluginRef {
            plugin_id: "plugin-browser".to_string(),
            selected_skill_ids: Vec::new(),
            selected_command_ids: Vec::new(),
            selected_agent_ids: Vec::new(),
        }],
        command_invocations: Vec::new(),
    };

    policy
        .apply_to_task(&mut task)
        .expect("apply Plugin UI policy");
    let snapshots = policy.plugin_snapshots(&task).expect("Plugin UI snapshots");
    assert_eq!(snapshots.len(), 1);
    assert_eq!(snapshots[0].component_snapshots.len(), 1);
    let component = &snapshots[0].component_snapshots[0];
    assert_eq!(component.kind, PluginComponentKind::UiContribution);
    assert_eq!(component.component_key, "security-workbench");
    assert_eq!(component.content_sha256, "c".repeat(64));
    assert_eq!(
        component.runtime.get("runtime_kind"),
        Some(&json!("sandboxed_ui"))
    );
    assert_eq!(
        component.runtime.get("entrypoint"),
        Some(&json!("./ui/index.html"))
    );
    assert_eq!(
        component
            .runtime
            .get("metadata")
            .and_then(|metadata| metadata.get("assets")),
        Some(&json!(["./ui/app.js", "./ui/styles.css"]))
    );
    assert_eq!(snapshots[0].permission_snapshot, vec!["artifact.read"]);
}

#[test]
fn plugin_selection_without_device_fails_closed() {
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
    let config = TaskPluginConfig {
        selected_plugins: vec![SelectedPluginRef {
            plugin_id: "plugin-browser".to_string(),
            selected_skill_ids: Vec::new(),
            selected_command_ids: Vec::new(),
            selected_agent_ids: Vec::new(),
        }],
        ..TaskPluginConfig::default()
    };

    let error = policy
        .validate_plugin_config(&config)
        .expect_err("device-less Plugin selection must fail closed");
    assert!(error.contains("device_id"));
}

#[test]
fn selected_command_enters_the_immutable_run_snapshot() {
    let mut capabilities = local_runtime_capabilities();
    capabilities.plugins = vec![resolved_command_plugin(false)];
    let policy = local_runtime_policy(capabilities).expect("Command Plugin policy");
    let mut task = task();
    task.plugin_config = TaskPluginConfig {
        device_id: Some("device-1".to_string()),
        workspace_id: Some("workspace-1".to_string()),
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
    let snapshots = policy
        .plugin_snapshots(&task)
        .expect("Command Plugin snapshots");

    assert_eq!(snapshots.len(), 1);
    assert_eq!(snapshots[0].component_snapshots.len(), 1);
    let component = &snapshots[0].component_snapshots[0];
    assert_eq!(component.component_key, "review");
    assert_eq!(component.kind, PluginComponentKind::Command);
    assert_eq!(component.content_sha256, "d".repeat(64));
    assert_eq!(
        component.runtime.get("entrypoint"),
        Some(&json!("./commands/review.md"))
    );
    assert_eq!(
        component
            .runtime
            .get("metadata")
            .and_then(|metadata| metadata.get("description")),
        Some(&json!("Review the current change"))
    );
    assert_eq!(
        component
            .runtime
            .get("metadata")
            .and_then(|metadata| metadata.get("argument_hint")),
        Some(&json!("[path]"))
    );
    assert_eq!(
        component
            .runtime
            .get("metadata")
            .and_then(|metadata| metadata.get("requires_confirmation")),
        Some(&json!(false))
    );
    assert_eq!(
        component
            .runtime
            .get("metadata")
            .and_then(|metadata| metadata.get("target_agent")),
        Some(&json!("task_runner_run_phase"))
    );
    assert_eq!(
        component
            .runtime
            .get("metadata")
            .and_then(|metadata| metadata.get("allowed_tools")),
        Some(&json!(["browser_tools_browser_snapshot"]))
    );
    assert_eq!(
        component.runtime.get("arguments"),
        Some(&json!("src/lib.rs"))
    );
    assert_eq!(snapshots[0].permission_snapshot, vec!["workspace.read"]);
}

#[test]
fn selected_agent_enters_the_immutable_run_snapshot_and_catalog() {
    let mut capabilities = local_runtime_capabilities();
    capabilities.plugins = vec![resolved_agent_plugin("task_runner_run_phase")];
    let policy = local_runtime_policy(capabilities).expect("Agent Plugin policy");
    let views = policy.selectable_plugin_views();
    assert_eq!(views[0].agents.len(), 1);
    assert_eq!(views[0].agents[0].agent_id, "reviewer");
    assert_eq!(views[0].agents[0].max_iterations, 12);

    let mut task = task();
    task.plugin_config = TaskPluginConfig {
        device_id: Some("device-1".to_string()),
        workspace_id: Some("workspace-1".to_string()),
        selected_plugins: vec![SelectedPluginRef {
            plugin_id: "plugin-browser".to_string(),
            selected_skill_ids: Vec::new(),
            selected_command_ids: Vec::new(),
            selected_agent_ids: vec!["reviewer".to_string()],
        }],
        command_invocations: Vec::new(),
    };
    policy
        .apply_to_task(&mut task)
        .expect("apply Agent Plugin policy");
    let snapshots = policy
        .plugin_snapshots(&task)
        .expect("Agent Plugin snapshots");
    assert_eq!(snapshots[0].component_snapshots.len(), 1);
    let component = &snapshots[0].component_snapshots[0];
    assert_eq!(component.component_key, "reviewer");
    assert_eq!(component.kind, PluginComponentKind::Agent);
    assert_eq!(component.content_sha256, "e".repeat(64));
    assert_eq!(
        component
            .runtime
            .get("metadata")
            .and_then(|metadata| metadata.get("base_agent")),
        Some(&json!("task_runner_run_phase"))
    );
    assert_eq!(
        component
            .runtime
            .get("metadata")
            .and_then(|metadata| metadata.get("max_iterations")),
        Some(&json!(12))
    );
}

#[test]
fn hook_set_is_automatically_bound_to_the_immutable_run_snapshot() {
    let mut capabilities = local_runtime_capabilities();
    capabilities.plugins = vec![resolved_hook_plugin()];
    let policy = local_runtime_policy(capabilities).expect("Hook Plugin policy");
    let mut task = task();
    task.plugin_config = TaskPluginConfig {
        device_id: Some("device-1".to_string()),
        workspace_id: Some("workspace-1".to_string()),
        selected_plugins: vec![SelectedPluginRef {
            plugin_id: "plugin-browser".to_string(),
            selected_skill_ids: Vec::new(),
            selected_command_ids: Vec::new(),
            selected_agent_ids: Vec::new(),
        }],
        command_invocations: Vec::new(),
    };

    policy.apply_to_task(&mut task).expect("apply Hook policy");
    let snapshots = policy.plugin_snapshots(&task).expect("Hook snapshots");
    assert_eq!(snapshots[0].component_snapshots.len(), 1);
    let component = &snapshots[0].component_snapshots[0];
    assert_eq!(component.kind, PluginComponentKind::HookSet);
    assert_eq!(component.component_key, "lifecycle-hooks");
    assert_eq!(component.content_sha256, "f".repeat(64));
    assert_eq!(
        component.runtime.get("entrypoint"),
        Some(&json!("./hooks.json"))
    );
    assert_eq!(snapshots[0].permission_snapshot, vec!["process.spawn"]);
}

#[test]
fn plugin_agent_must_match_the_existing_plan_or_run_agent() {
    let mut run_capabilities = local_runtime_capabilities();
    run_capabilities.plugins = vec![resolved_agent_plugin("task_runner_plan_phase")];
    let run_policy = local_runtime_policy(run_capabilities)
        .expect("incompatible optional Agent components are filtered");
    assert!(run_policy.selectable_plugin_views().is_empty());

    let mut plan_capabilities = local_runtime_capabilities();
    plan_capabilities.agent_key = SystemAgentKey::TaskRunnerPlanPhase.as_str().to_string();
    plan_capabilities.plugins = vec![resolved_agent_plugin("task_runner_plan_phase")];
    let policy = local_runtime_policy(plan_capabilities).expect("plan Agent Plugin policy");
    assert_eq!(
        policy.selectable_plugin_views()[0].agents[0].agent_id,
        "reviewer"
    );
}

#[test]
fn a_task_may_select_only_one_plugin_agent() {
    let mut capabilities = local_runtime_capabilities();
    capabilities.plugins = vec![resolved_agent_plugin("task_runner_run_phase")];
    let policy = local_runtime_policy(capabilities).expect("Agent Plugin policy");
    let config = TaskPluginConfig {
        device_id: Some("device-1".to_string()),
        selected_plugins: vec![SelectedPluginRef {
            plugin_id: "plugin-browser".to_string(),
            selected_skill_ids: Vec::new(),
            selected_command_ids: Vec::new(),
            selected_agent_ids: vec!["reviewer".to_string(), "second".to_string()],
        }],
        ..TaskPluginConfig::default()
    };
    assert!(policy
        .validate_plugin_config(&config)
        .expect_err("multiple Plugin Agents must fail")
        .contains("more than one Agent"));
}

#[test]
fn command_requiring_confirmation_is_preserved_for_local_device_approval() {
    let mut capabilities = local_runtime_capabilities();
    capabilities.plugins = vec![resolved_command_plugin(true)];
    let policy = local_runtime_policy(capabilities).expect("Command Plugin policy");
    let config = TaskPluginConfig {
        device_id: Some("device-1".to_string()),
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
        device_id: Some("device-1".to_string()),
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
        ..TaskPluginConfig::default()
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
        .insert("target_agent".to_string(), json!("task_runner_plan_phase"));
    let mut capabilities = local_runtime_capabilities();
    capabilities.plugins = vec![command_plugin];
    let policy = local_runtime_policy(capabilities).expect("Command Plugin policy");
    let config = TaskPluginConfig {
        device_id: Some("device-1".to_string()),
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
    task.plugin_config.device_id = Some("device-1".to_string());

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
fn cloud_runtime_injects_enabled_agent_plugins_without_user_selection() {
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
    let policy = TaskRunnerCapabilityPolicy::new_for_runtime(capabilities, false)
        .expect("cloud Plugin policy");
    let mut task = task();

    policy
        .apply_to_task(&mut task)
        .expect("apply cloud Plugin policy");
    let snapshots = policy.plugin_snapshots(&task).expect("Plugin snapshots");

    assert_eq!(task.plugin_config.selected_plugins.len(), 1);
    assert_eq!(
        task.plugin_config.selected_plugins[0].plugin_id,
        "plugin-browser"
    );
    assert_eq!(snapshots.len(), 1);
    assert!(snapshots[0].device_id.is_none());
}
