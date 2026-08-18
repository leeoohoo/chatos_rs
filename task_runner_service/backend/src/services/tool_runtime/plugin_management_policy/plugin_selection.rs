// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::{HashMap, HashSet};

use chatos_plugin_management_sdk::{
    PluginComponentDescriptor, PluginComponentKind, ResolvedPlugin, SelectedPluginRef,
};
use serde_json::Value;

use super::{is_task_runner_execution_agent, normalized_unique_ids};

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
    if release.plugin_id != plugin.catalog.id {
        return Err(format!(
            "Plugin Release does not match the Catalog identity: {}",
            plugin.catalog.id
        ));
    }

    let available_skills = plugin
        .components
        .iter()
        .filter(|component| {
            component.available
                && component.component.kind == PluginComponentKind::SkillCollection
                && plugin_component_supported_for_agent(&component.component, expected_agent)
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
            expected_agent,
        ));
    }

    Ok(())
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
        PluginComponentKind::Agent => false,
        PluginComponentKind::HookSet | PluginComponentKind::UiContribution => false,
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
