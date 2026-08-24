// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashSet;

use chatos_mcp_runtime::{builtin_kind_by_any, complete_builtin_kind_dependencies};
use chatos_plugin_management_sdk::SelectedPluginRef;

use crate::models::{is_reserved_internal_mcp_resource_id, TaskRecord};

use super::{
    dedupe_builtin_kinds, normalized_command_invocation, normalized_unique_ids,
    plugin_builtin_kind, plugin_task_process_log_mcp, TaskRunnerCapabilityPolicy,
};

impl TaskRunnerCapabilityPolicy {
    pub(crate) fn apply_to_task(&self, task: &mut TaskRecord) -> Result<(), String> {
        self.capabilities
            .ensure_required_available()
            .map_err(|err| err.to_string())?;
        self.validate_optional_config(&task.mcp_config)?;
        task.mcp_config.enabled = true;

        let mut effective_builtin = task
            .mcp_config
            .enabled_builtin_kinds
            .iter()
            .filter_map(|value| builtin_kind_by_any(value))
            .chain(
                self.capabilities
                    .required_mcps()
                    .filter_map(plugin_builtin_kind),
            )
            .collect::<Vec<_>>();
        effective_builtin = complete_builtin_kind_dependencies(effective_builtin);
        dedupe_builtin_kinds(&mut effective_builtin);
        effective_builtin.sort_by_key(|kind| kind.kind_name());
        task.mcp_config.enabled_builtin_kinds = effective_builtin
            .into_iter()
            .map(|kind| kind.kind_name().to_string())
            .collect();

        let mut effective_external = task.mcp_config.external_mcp_config_ids.clone();
        effective_external.extend(
            self.capabilities
                .required_mcps()
                .filter(|item| {
                    plugin_builtin_kind(item).is_none() && !plugin_task_process_log_mcp(item)
                })
                .filter(|item| !is_reserved_internal_mcp_resource_id(item.resource.id.as_str()))
                .map(|item| item.resource.id.clone()),
        );
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
        task.plugin_config.selected_plugins = effective;
        task.plugin_config.command_invocations = command_invocations;
        Ok(())
    }
}
