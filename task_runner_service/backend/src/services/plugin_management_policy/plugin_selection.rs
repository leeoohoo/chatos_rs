// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::{BTreeMap, HashMap, HashSet};

use chatos_plugin_management_sdk::{
    PluginCommandInvocation, PluginComponentDescriptor, PluginComponentKind, ResolvedPlugin,
    RunPluginComponentSnapshot, RunPluginSnapshot, SelectedPluginRef,
};
use serde_json::Value;

use super::{
    is_task_runner_execution_agent, normalized_optional_plugin_text, normalized_unique_ids,
};

pub(super) fn validate_supported_plugin(
    plugin: &ResolvedPlugin,
    expected_agent: &str,
) -> Result<(), String> {
    let mut supported = 0usize;
    for component in plugin
        .components
        .iter()
        .filter(|component| component.available)
    {
        if plugin_component_supported_for_agent(&component.component, expected_agent) {
            supported += 1;
        } else if component.component.required {
            return Err(format!(
                "required Plugin component runtime is not implemented for {expected_agent}: {}:{}",
                plugin.catalog.id, component.component.component_key
            ));
        }
    }
    if supported == 0 {
        return Err(format!(
            "Plugin has no Task Runner-supported components: {}",
            plugin.catalog.id
        ));
    }
    Ok(())
}

pub(super) fn validate_plugin_component_selection(
    plugin: &ResolvedPlugin,
    selected: &SelectedPluginRef,
    expected_agent: &str,
) -> Result<(), String> {
    validate_supported_plugin(plugin, expected_agent)?;
    let release = plugin
        .release
        .as_ref()
        .ok_or_else(|| format!("Plugin Release snapshot is missing: {}", plugin.catalog.id))?;
    let installation = plugin.installation.as_ref().ok_or_else(|| {
        format!(
            "Plugin installation snapshot is missing: {}",
            plugin.catalog.id
        )
    })?;
    if release.plugin_id != plugin.catalog.id
        || installation.plugin_id != plugin.catalog.id
        || installation.release_id != release.id
        || installation.version != release.version
        || installation.artifact_sha256 != release.artifact_sha256
        || !installation.active
    {
        return Err(format!(
            "Plugin installation does not match the active immutable Release: {}",
            plugin.catalog.id
        ));
    }

    let available_skills = plugin
        .components
        .iter()
        .filter(|component| {
            component.available
                && component.component.kind == PluginComponentKind::SkillCollection
                && is_task_runner_execution_agent(expected_agent)
        })
        .map(|component| plugin_skill_id(&component.component))
        .collect::<HashSet<_>>();
    for skill_id in normalized_unique_ids(&selected.selected_skill_ids, "selected_skill_ids")? {
        if !available_skills.contains(skill_id.as_str()) {
            return Err(format!(
                "Plugin Skill is not available in {}: {skill_id}",
                plugin.catalog.id
            ));
        }
    }

    let available_commands = plugin
        .components
        .iter()
        .filter(|component| {
            component.available
                && component.component.kind == PluginComponentKind::Command
                && plugin_component_supported_for_agent(&component.component, expected_agent)
        })
        .map(|component| (component.component.component_key.as_str(), component))
        .collect::<HashMap<_, _>>();
    let selected_commands =
        normalized_unique_ids(&selected.selected_command_ids, "selected_command_ids")?;
    for command_id in &selected_commands {
        let command = available_commands.get(command_id.as_str()).ok_or_else(|| {
            format!(
                "Plugin Command is not available or is incompatible with {expected_agent} in {}: {command_id}",
                plugin.catalog.id,
            )
        })?;
        debug_assert!(plugin_component_supported_for_agent(
            &command.component,
            expected_agent
        ));
    }

    let available_agents = plugin
        .components
        .iter()
        .filter(|component| {
            component.available
                && component.component.kind == PluginComponentKind::Agent
                && plugin_component_supported_for_agent(&component.component, expected_agent)
        })
        .map(|component| component.component.component_key.as_str())
        .collect::<HashSet<_>>();
    let selected_agents =
        normalized_unique_ids(&selected.selected_agent_ids, "selected_agent_ids")?;
    if selected_agents.len() > 1 {
        return Err(format!(
            "Plugin selects more than one Agent: {}",
            plugin.catalog.id
        ));
    }
    for agent_id in &selected_agents {
        if !available_agents.contains(agent_id.as_str()) {
            return Err(format!(
                "Plugin Agent is not available for {expected_agent} in {}: {agent_id}",
                plugin.catalog.id
            ));
        }
    }
    Ok(())
}

pub(super) fn plugin_snapshot(
    plugin: &ResolvedPlugin,
    selected: &SelectedPluginRef,
    workspace_id: Option<&str>,
    command_invocations: &[PluginCommandInvocation],
    expected_agent: &str,
) -> Result<RunPluginSnapshot, String> {
    validate_plugin_component_selection(plugin, selected, expected_agent)?;
    let release = plugin
        .release
        .as_ref()
        .ok_or_else(|| format!("Plugin Release snapshot is missing: {}", plugin.catalog.id))?;
    let installation = plugin.installation.as_ref().ok_or_else(|| {
        format!(
            "Plugin installation snapshot is missing: {}",
            plugin.catalog.id
        )
    })?;
    let selected_skill_ids =
        normalized_unique_ids(&selected.selected_skill_ids, "selected_skill_ids")?
            .into_iter()
            .collect::<HashSet<_>>();
    let selected_command_ids =
        normalized_unique_ids(&selected.selected_command_ids, "selected_command_ids")?
            .into_iter()
            .collect::<HashSet<_>>();
    let selected_agent_ids =
        normalized_unique_ids(&selected.selected_agent_ids, "selected_agent_ids")?
            .into_iter()
            .collect::<HashSet<_>>();
    let snapshots_by_key = plugin
        .component_snapshots
        .iter()
        .map(|snapshot| (snapshot.component.component_key.as_str(), snapshot))
        .collect::<HashMap<_, _>>();
    let mut selected_component_keys = HashSet::new();
    let mut component_snapshots = Vec::new();
    for component in plugin
        .components
        .iter()
        .filter(|component| component.available)
    {
        let include = match component.component.kind {
            PluginComponentKind::SkillCollection => {
                is_task_runner_execution_agent(expected_agent)
                    && (selected_skill_ids.is_empty()
                        || selected_skill_ids
                            .contains(plugin_skill_id(&component.component).as_str())
                        || component.component.required)
            }
            PluginComponentKind::McpServer => is_task_runner_execution_agent(expected_agent),
            PluginComponentKind::Command => {
                selected_command_ids.contains(component.component.component_key.as_str())
            }
            PluginComponentKind::Agent => {
                selected_agent_ids.contains(component.component.component_key.as_str())
            }
            PluginComponentKind::HookSet => is_task_runner_execution_agent(expected_agent),
            PluginComponentKind::UiContribution => is_task_runner_execution_agent(expected_agent),
            _ => component.component.required,
        };
        if !include {
            continue;
        }
        if !matches!(
            component.component.kind,
            PluginComponentKind::SkillCollection
                | PluginComponentKind::McpServer
                | PluginComponentKind::Command
                | PluginComponentKind::Agent
                | PluginComponentKind::HookSet
                | PluginComponentKind::UiContribution
        ) {
            return Err(format!(
                "Plugin component runtime is not implemented yet: {}:{}",
                plugin.catalog.id, component.component.component_key
            ));
        }
        let immutable = snapshots_by_key
            .get(component.component.component_key.as_str())
            .ok_or_else(|| {
                format!(
                    "immutable Plugin component snapshot is missing: {}:{}",
                    plugin.catalog.id, component.component.component_key
                )
            })?;
        if immutable.plugin_id != plugin.catalog.id
            || immutable.release_id != release.id
            || immutable.component != component.component
        {
            return Err(format!(
                "Plugin component snapshot does not match the resolved Release: {}:{}",
                plugin.catalog.id, component.component.component_key
            ));
        }
        let mut runtime = BTreeMap::new();
        runtime.insert(
            "runtime_kind".to_string(),
            Value::String(component.component.runtime_kind.clone()),
        );
        if let Some(entrypoint) = component.component.entrypoint.as_ref() {
            runtime.insert(
                "entrypoint".to_string(),
                Value::String(entrypoint.path.clone()),
            );
        }
        if !component.component.metadata.is_empty() {
            runtime.insert(
                "metadata".to_string(),
                Value::Object(component.component.metadata.clone().into_iter().collect()),
            );
        }
        if component.component.kind == PluginComponentKind::SkillCollection {
            runtime.insert(
                "skill_keys".to_string(),
                Value::Array(vec![Value::String(
                    component.component.component_key.clone(),
                )]),
            );
        }
        if component.component.kind == PluginComponentKind::Command {
            if let Some(arguments) = command_invocations
                .iter()
                .find(|invocation| {
                    invocation.plugin_id == plugin.catalog.id
                        && invocation.command_id == component.component.component_key
                })
                .and_then(|invocation| invocation.arguments.as_ref())
            {
                runtime.insert("arguments".to_string(), Value::String(arguments.clone()));
            }
        }
        selected_component_keys.insert(component.component.component_key.as_str());
        component_snapshots.push(RunPluginComponentSnapshot {
            component_key: component.component.component_key.clone(),
            kind: component.component.kind,
            content_sha256: immutable.content_sha256.clone(),
            runtime,
        });
    }
    if component_snapshots.is_empty() {
        return Err(format!(
            "effective Plugin component selection is empty: {}",
            plugin.catalog.id
        ));
    }
    component_snapshots.sort_by(|left, right| left.component_key.cmp(&right.component_key));

    let mut permission_snapshot = release
        .permissions
        .iter()
        .filter(|permission| {
            permission.components.is_empty()
                || permission
                    .components
                    .iter()
                    .any(|key| selected_component_keys.contains(key.as_str()))
        })
        .map(|permission| permission.permission.trim().to_string())
        .filter(|permission| !permission.is_empty())
        .collect::<Vec<_>>();
    permission_snapshot.extend(
        plugin
            .components
            .iter()
            .filter(|component| {
                selected_component_keys.contains(component.component.component_key.as_str())
            })
            .flat_map(|component| component.component.permissions.iter())
            .map(|permission| permission.permission.trim().to_string())
            .filter(|permission| !permission.is_empty()),
    );
    permission_snapshot.sort();
    permission_snapshot.dedup();
    let mut auth_connection_ids = plugin.auth_connection_ids.clone();
    auth_connection_ids.sort();
    auth_connection_ids.dedup();

    Ok(RunPluginSnapshot {
        plugin_id: plugin.catalog.id.clone(),
        release_id: release.id.clone(),
        version: release.version.clone(),
        artifact_sha256: release.artifact_sha256.clone(),
        device_id: installation.device_id.clone(),
        workspace_id: normalized_optional_plugin_text(workspace_id, "workspace_id")?,
        component_snapshots,
        permission_snapshot,
        auth_connection_ids,
    })
}

fn plugin_component_supported_for_agent(
    component: &PluginComponentDescriptor,
    expected_agent: &str,
) -> bool {
    match component.kind {
        PluginComponentKind::SkillCollection | PluginComponentKind::McpServer => {
            is_task_runner_execution_agent(expected_agent)
        }
        PluginComponentKind::Command => {
            component
                .metadata
                .get("target_agent")
                .and_then(Value::as_str)
                .is_some_and(|target_agent| target_agent == expected_agent)
                || (is_task_runner_execution_agent(expected_agent)
                    && !component.metadata.contains_key("target_agent"))
        }
        PluginComponentKind::Agent => {
            component.metadata.get("base_agent").and_then(Value::as_str) == Some(expected_agent)
        }
        PluginComponentKind::HookSet => is_task_runner_execution_agent(expected_agent),
        PluginComponentKind::UiContribution => is_task_runner_execution_agent(expected_agent),
        _ => false,
    }
}

fn plugin_skill_id(component: &PluginComponentDescriptor) -> String {
    component
        .metadata
        .get("skill_id")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(component.component_key.as_str())
        .to_string()
}
