// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_plugin_management_sdk::{
    PluginCommandInvocation, PluginComponentKind, ResolvedAgentCapabilities, SelectedPluginRef,
    SystemAgentKey, TaskPluginConfig,
};
use serde_json::json;

use crate::models::CreateTaskPluginHint;

use super::super::TaskRunnerCapabilityPolicy;
use super::fixtures::*;

const RUN_AGENT_KEY: &str = SystemAgentKey::TaskRunnerRunPhase.as_str();
const PLAN_AGENT_KEY: &str = SystemAgentKey::TaskRunnerPlanPhase.as_str();

fn local_runtime_capabilities() -> ResolvedAgentCapabilities {
    policy().capabilities
}

fn local_runtime_policy(
    capabilities: ResolvedAgentCapabilities,
) -> Result<TaskRunnerCapabilityPolicy, String> {
    TaskRunnerCapabilityPolicy::new(capabilities, local_runtime_context())
}

#[test]
fn trusted_plugin_hints_resolve_plugin_key_to_internal_id() {
    let mut capabilities = local_runtime_capabilities();
    capabilities.plugins = vec![resolved_plugin(false)];
    let policy = local_runtime_policy(capabilities).expect("Plugin policy");
    let selection = policy
        .plugin_selection_from_hints(&[CreateTaskPluginHint {
            plugin_key: "browser@official".to_string(),
            reason: Some("Task needs browser control".to_string()),
        }])
        .expect("trusted Plugin selection");
    let config = selection.plugin_config;

    assert_eq!(config.selected_plugins.len(), 1);
    assert_eq!(config.selected_plugins[0].plugin_id, "plugin-browser");
    let audit = selection.audit.expect("Plugin selection audit");
    assert_eq!(audit.policy_revision, "revision-1");
    assert_eq!(audit.project_context_revision, "project-revision-1");
    assert_eq!(audit.plugins[0].plugin_key, "browser@official");
    assert_eq!(audit.plugins[0].display_name, "Browser");
    assert_eq!(audit.plugins[0].device_id, "device-1");
    assert_eq!(
        audit.plugins[0].reason.as_deref(),
        Some("Task needs browser control")
    );
}

#[test]
fn trusted_plugin_hints_reject_unknown_plugin_key() {
    let mut capabilities = local_runtime_capabilities();
    capabilities.plugins = vec![resolved_plugin(false)];
    let policy = local_runtime_policy(capabilities).expect("Plugin policy");
    let error = policy
        .plugin_selection_from_hints(&[CreateTaskPluginHint {
            plugin_key: "unknown-plugin".to_string(),
            reason: None,
        }])
        .expect_err("unknown Plugin hint must fail closed");

    assert!(error.contains("not selectable"));
}

#[test]
fn plugin_hints_require_a_local_connector_device() {
    let mut capabilities = local_runtime_capabilities();
    capabilities.plugins = vec![resolved_plugin(false)];
    let policy = TaskRunnerCapabilityPolicy::new(
        capabilities,
        crate::services::task_plugin_runtime_context::TaskPluginRuntimeContext::server(
            "owner-1", "public",
        ),
    )
    .expect("server capability policy");

    assert!(policy.selectable_plugins().is_empty());
    let error = policy
        .plugin_selection_from_hints(&[CreateTaskPluginHint {
            plugin_key: "browser@official".to_string(),
            reason: None,
        }])
        .expect_err("Plugin selection without a local device must fail closed");

    assert!(error.contains("Local Connector project device"));
}

#[test]
fn selected_plugin_can_require_a_device_exclusive_execution_lane() {
    let mut plugin = resolved_plugin(false);
    plugin.components[0]
        .component
        .metadata
        .insert("requires_exclusive_execution".to_string(), json!(true));
    let mut capabilities = local_runtime_capabilities();
    capabilities.plugins = vec![plugin];
    let policy = local_runtime_policy(capabilities).expect("Plugin policy");
    let config = TaskPluginConfig {
        selected_plugins: vec![SelectedPluginRef {
            plugin_id: "plugin-browser".to_string(),
            selected_skill_ids: Vec::new(),
            selected_command_ids: Vec::new(),
            selected_agent_ids: Vec::new(),
        }],
        command_invocations: Vec::new(),
    };

    assert_eq!(
        policy
            .exclusive_execution_lane_key(&config)
            .expect("exclusive lane"),
        Some("plugin-exclusive-device:owner-1:device-1".to_string())
    );
}

#[test]
fn ordinary_selected_plugin_does_not_require_an_exclusive_execution_lane() {
    let mut capabilities = local_runtime_capabilities();
    capabilities.plugins = vec![resolved_plugin(false)];
    let policy = local_runtime_policy(capabilities).expect("Plugin policy");
    let config = TaskPluginConfig {
        selected_plugins: vec![SelectedPluginRef {
            plugin_id: "plugin-browser".to_string(),
            selected_skill_ids: Vec::new(),
            selected_command_ids: Vec::new(),
            selected_agent_ids: Vec::new(),
        }],
        command_invocations: Vec::new(),
    };

    assert_eq!(
        policy
            .exclusive_execution_lane_key(&config)
            .expect("ordinary lane"),
        None
    );
}

#[test]
fn run_validation_rejects_plugin_release_drift() {
    let mut capabilities = local_runtime_capabilities();
    capabilities.plugins = vec![resolved_plugin(false)];
    let policy = local_runtime_policy(capabilities).expect("Plugin policy");
    let selection = policy
        .plugin_selection_from_hints(&[CreateTaskPluginHint {
            plugin_key: "browser@official".to_string(),
            reason: Some("Task needs browser control".to_string()),
        }])
        .expect("trusted Plugin selection");
    let mut task = task();
    task.plugin_config = selection.plugin_config;
    task.plugin_selection_audit = selection.audit;

    policy
        .validate_task_plugin_selection_for_run(&task)
        .expect("unchanged Plugin snapshot");
    task.plugin_selection_audit.as_mut().expect("audit").plugins[0].version = "2.0.0".to_string();
    let error = policy
        .validate_task_plugin_selection_for_run(&task)
        .expect_err("Release drift must fail closed");

    assert!(error.contains("Plugin release changed"));
}

#[test]
fn manual_retry_refresh_rebinds_to_the_current_installed_release() {
    let mut initial_capabilities = local_runtime_capabilities();
    initial_capabilities.plugins = vec![resolved_plugin(false)];
    let initial_policy = local_runtime_policy(initial_capabilities).expect("initial Plugin policy");
    let selection = initial_policy
        .plugin_selection_from_hints(&[CreateTaskPluginHint {
            plugin_key: "browser@official".to_string(),
            reason: Some("Task needs browser control".to_string()),
        }])
        .expect("initial trusted Plugin selection");
    let mut task = task();
    task.plugin_config = selection.plugin_config;
    task.plugin_selection_audit = selection.audit;

    let mut upgraded_plugin = resolved_plugin(false);
    upgraded_plugin.catalog.latest_release_id = "release-browser-2".to_string();
    let release = upgraded_plugin.release.as_mut().expect("upgraded release");
    release.id = "release-browser-2".to_string();
    release.version = "2.0.0".to_string();
    release.normalized_manifest.version = "2.0.0".to_string();
    release.npm_package.version = "2.0.0".to_string();
    release.artifact_sha256 = "d".repeat(64);
    let installation = upgraded_plugin
        .installation
        .as_mut()
        .expect("upgraded installation");
    installation.release_id = release.id.clone();
    installation.version = release.version.clone();
    installation.artifact_sha256 = release.artifact_sha256.clone();
    for component in &mut upgraded_plugin.component_snapshots {
        component.release_id = release.id.clone();
    }
    let mut upgraded_capabilities = local_runtime_capabilities();
    upgraded_capabilities.plugins = vec![upgraded_plugin];
    let upgraded_policy =
        local_runtime_policy(upgraded_capabilities).expect("upgraded Plugin policy");

    assert!(upgraded_policy
        .validate_task_plugin_selection_for_run(&task)
        .expect_err("normal run must still reject release drift")
        .contains("Plugin release changed"));
    assert!(upgraded_policy
        .refresh_task_plugin_selection_for_manual_retry(&mut task)
        .expect("manual retry refresh"));

    let audit = task
        .plugin_selection_audit
        .as_ref()
        .expect("refreshed audit");
    assert_eq!(audit.selection_source, "manual_retry_refresh");
    assert_eq!(audit.plugins[0].release_id, "release-browser-2");
    assert_eq!(audit.plugins[0].version, "2.0.0");
    assert_eq!(
        audit.plugins[0].reason.as_deref(),
        Some("Task needs browser control")
    );
    upgraded_policy
        .validate_task_plugin_selection_for_run(&task)
        .expect("refreshed task must validate");
    assert!(!upgraded_policy
        .refresh_task_plugin_selection_for_manual_retry(&mut task)
        .expect("idempotent manual retry refresh"));
}

#[test]
fn plugin_selection_omits_plugin_agent_profiles() {
    let mut capabilities = local_runtime_capabilities();
    capabilities.plugins = vec![resolved_plugin(false)];
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
    let policy = local_runtime_policy(capabilities).expect("required Plugin policy");
    let mut task = task();
    assert!(policy
        .validate_task_plugin_selection_for_run(&task)
        .expect_err("required Plugin without an audit snapshot must fail closed")
        .contains("audit snapshot is missing"));
    let selection = policy
        .plugin_selection_from_hints(&[])
        .expect("required Plugin trusted selection");
    task.plugin_config = selection.plugin_config;
    task.plugin_selection_audit = selection.audit;

    policy
        .validate_task_plugin_selection_for_run(&task)
        .expect("validate required Plugin snapshot");
    policy
        .apply_to_task(&mut task)
        .expect("apply required Plugin");

    assert_eq!(task.plugin_config.selected_plugins.len(), 1);
    assert_eq!(
        task.plugin_config.selected_plugins[0].plugin_id,
        "plugin-browser"
    );
}
