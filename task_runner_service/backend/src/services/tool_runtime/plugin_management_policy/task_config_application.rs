// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashSet;

use chatos_mcp_runtime::{builtin_kind_by_any, BuiltinMcpKind};
use chatos_plugin_management_sdk::SelectedPluginRef;

use crate::models::TaskRecord;

use super::{
    dedupe_builtin_kinds, normalized_command_invocation, normalized_unique_ids,
    plugin_builtin_kind, plugin_task_process_log_mcp, TaskRunnerCapabilityPolicy,
};

impl TaskRunnerCapabilityPolicy {
    pub(crate) fn apply_to_task(&self, task: &mut TaskRecord) -> Result<(), String> {
        self.capabilities
            .ensure_required_available()
            .map_err(|err| err.to_string())?;
        task.mcp_config.enabled = true;

        let mut effective_builtin = self
            .capabilities
            .mcps
            .iter()
            .filter(|item| item.binding.enabled && item.resource.enabled)
            .filter_map(plugin_builtin_kind)
            .collect::<Vec<_>>();
        dedupe_builtin_kinds(&mut effective_builtin);
        task.mcp_config.enabled_builtin_kinds = effective_builtin
            .into_iter()
            .map(|kind| kind.kind_name().to_string())
            .collect();

        let mut effective_external = self
            .capabilities
            .mcps
            .iter()
            .filter(|item| {
                item.binding.enabled
                    && item.resource.enabled
                    && plugin_builtin_kind(item).is_none()
                    && !plugin_task_process_log_mcp(item)
            })
            .map(|item| item.resource.id.clone())
            .collect::<Vec<_>>();
        effective_external.sort();
        effective_external.dedup();
        task.mcp_config.external_mcp_config_ids = effective_external;
        let mut effective_skills = Vec::new();
        effective_skills.extend(
            self.capabilities
                .skills
                .iter()
                .filter(|item| item.binding.required && item.available)
                .map(|item| item.resource.id.clone()),
        );
        effective_skills.sort();
        effective_skills.dedup();
        task.mcp_config.selected_skill_ids = effective_skills;
        task.mcp_config.skill_policy_revision = Some(self.policy_revision().to_string());
        task.mcp_config.sandbox_manager_base_url = None;
        task.mcp_config.ephemeral_http_servers.clear();
        self.apply_plugins_to_task(task)?;
        Ok(())
    }

    pub(crate) fn apply_plugins_to_task(&self, task: &mut TaskRecord) -> Result<(), String> {
        // Legacy task snapshots may contain Plugin Agent selections. They no longer participate
        // in execution because the system Agent is fixed by the authenticated MCP context.
        for selected in &mut task.plugin_config.selected_plugins {
            selected.selected_agent_ids.clear();
        }
        self.validate_plugin_config(&task.plugin_config)?;
        let allowed_effective = self
            .selectable_plugins()
            .into_iter()
            .map(|plugin| plugin.catalog.id.as_str())
            .chain(
                self.capabilities
                    .required_plugins()
                    .filter(|plugin| {
                        plugin.available
                            || plugin.status
                                == chatos_plugin_management_sdk::PluginAvailabilityStatus::PartiallyAvailable
                    })
                    .map(|plugin| plugin.catalog.id.as_str()),
            )
            .collect::<HashSet<_>>();
        let mut effective = Vec::new();
        for selected in &task.plugin_config.selected_plugins {
            let plugin_id = selected.plugin_id.trim();
            if allowed_effective.contains(plugin_id) {
                effective.push(SelectedPluginRef {
                    plugin_id: plugin_id.to_string(),
                    selected_skill_ids: normalized_unique_ids(
                        &selected.selected_skill_ids,
                        "selected_skill_ids",
                    )?,
                    selected_command_ids: normalized_unique_ids(
                        &selected.selected_command_ids,
                        "selected_command_ids",
                    )?,
                    selected_agent_ids: Vec::new(),
                });
            }
        }
        if !self.portable_uses_local {
            for plugin in self.selectable_plugins() {
                if !effective
                    .iter()
                    .any(|selected| selected.plugin_id == plugin.catalog.id)
                {
                    effective.push(SelectedPluginRef {
                        plugin_id: plugin.catalog.id.clone(),
                        selected_skill_ids: Vec::new(),
                        selected_command_ids: Vec::new(),
                        selected_agent_ids: Vec::new(),
                    });
                }
            }
        }
        for plugin in self.capabilities.required_plugins().filter(|plugin| {
            plugin.available
                || plugin.status
                    == chatos_plugin_management_sdk::PluginAvailabilityStatus::PartiallyAvailable
        }) {
            if !effective
                .iter()
                .any(|selected| selected.plugin_id == plugin.catalog.id)
            {
                effective.push(SelectedPluginRef {
                    plugin_id: plugin.catalog.id.clone(),
                    selected_skill_ids: Vec::new(),
                    selected_command_ids: Vec::new(),
                    selected_agent_ids: Vec::new(),
                });
            }
        }
        effective.sort_by(|left, right| left.plugin_id.cmp(&right.plugin_id));
        let command_invocations = task
            .plugin_config
            .command_invocations
            .iter()
            .map(normalized_command_invocation)
            .collect::<Result<Vec<_>, _>>()?;
        // Execution location belongs to the project and is frozen into the Run snapshot.
        task.plugin_config.device_id = None;
        task.plugin_config.workspace_id = None;
        task.plugin_config.selected_plugins = effective;
        task.plugin_config.command_invocations = command_invocations;
        self.inject_plugin_builtin_dependencies(task)?;
        Ok(())
    }

    fn inject_plugin_builtin_dependencies(&self, task: &mut TaskRecord) -> Result<(), String> {
        if self.is_planning_agent() {
            return Ok(());
        }
        let mut dependencies = Vec::new();
        for selected in &task.plugin_config.selected_plugins {
            let plugin = self
                .capabilities
                .plugins
                .iter()
                .find(|plugin| {
                    plugin.catalog.id == selected.plugin_id
                        && (plugin.available
                            || plugin.status
                                == chatos_plugin_management_sdk::PluginAvailabilityStatus::PartiallyAvailable)
                })
                .ok_or_else(|| {
                    format!("effective Plugin is unavailable: {}", selected.plugin_id)
                })?;
            if plugin.catalog.name == "browser" {
                dependencies.push(BuiltinMcpKind::BrowserTools);
            }
        }
        for dependency in dependencies {
            let available = self.capabilities.mcps.iter().any(|item| {
                item.binding.enabled
                    && item.resource.enabled
                    && plugin_builtin_kind(item) == Some(dependency)
            });
            if !available {
                return Err(format!(
                    "Plugin builtin dependency is unavailable for {}: {}",
                    self.capabilities.agent_key,
                    dependency.kind_name()
                ));
            }
            if !task
                .mcp_config
                .enabled_builtin_kinds
                .iter()
                .filter_map(|value| builtin_kind_by_any(value))
                .any(|kind| kind == dependency)
            {
                task.mcp_config
                    .enabled_builtin_kinds
                    .push(dependency.kind_name().to_string());
            }
        }
        dedupe_task_builtin_kind_names(&mut task.mcp_config.enabled_builtin_kinds);
        Ok(())
    }
}

fn dedupe_task_builtin_kind_names(values: &mut Vec<String>) {
    let mut kinds = values
        .iter()
        .filter_map(|value| builtin_kind_by_any(value))
        .collect::<Vec<_>>();
    dedupe_builtin_kinds(&mut kinds);
    *values = kinds
        .into_iter()
        .map(|kind| kind.kind_name().to_string())
        .collect();
}
