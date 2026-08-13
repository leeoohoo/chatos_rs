// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::{BTreeMap, HashSet};

use chatos_agent::{
    is_task_runner_execution_agent as is_task_runner_execution_key,
    is_task_runner_planning_agent as is_task_runner_planning_key, parse_system_agent_key,
};
use chatos_mcp::{system_mcp_descriptor_for_record, SystemMcpBackend, SystemMcpDescriptor};
use chatos_mcp_runtime::{builtin_kind_by_any, complete_builtin_kind_dependencies, BuiltinMcpKind};
use chatos_plugin_management_sdk::{
    PluginCommandInvocation, PluginManagementClient, ResolveAgentCapabilitiesRequest,
    ResolvedAgentCapabilities, ResolvedMcp, ResolvedPlugin, ResolvedSkill, SystemAgentKey,
    TaskPluginConfig,
};
use serde::Serialize;

use super::status_display::TaskScheduleModeExt;
use super::{RunService, TaskService};
use crate::auth::{get_current_access_token, CurrentUser};
use crate::models::{TaskMcpConfig, TaskRecord};

#[path = "plugin_management_policy/plugin_selection.rs"]
mod plugin_selection;
#[path = "plugin_management_policy/policy_resolution.rs"]
mod policy_resolution;
#[path = "plugin_management_policy/selectable_views.rs"]
pub(crate) mod selectable_views;
#[path = "plugin_management_policy/task_config_application.rs"]
mod task_config_application;

use plugin_selection::{validate_plugin_component_selection, validate_supported_plugin};

const MAX_PLUGIN_COMMAND_INVOCATIONS: usize = 64;
const MAX_PLUGIN_COMMAND_ARGUMENT_BYTES: usize = 16 * 1024;
#[cfg(test)]
const BUILTIN_RUNTIME_KIND: &str = chatos_plugin_management_sdk::LEGACY_BUILTIN_MCP_RUNTIME_KIND;

#[derive(Debug, Clone)]
pub(crate) struct TaskRunnerCapabilityPolicy {
    capabilities: ResolvedAgentCapabilities,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct TaskSkillSnapshotView {
    pub skill_id: String,
    pub bundle_id: String,
    pub version: String,
    pub bundle_hash: String,
    pub device_id: String,
    pub platform: String,
    pub entrypoint_kind: Option<String>,
}

impl TaskRunnerCapabilityPolicy {
    fn new(capabilities: ResolvedAgentCapabilities) -> Result<Self, String> {
        if !capabilities.agent_enabled {
            return Err(format!(
                "Task Runner Agent is disabled by Plugin Management: {}",
                capabilities.agent_key
            ));
        }
        capabilities
            .ensure_required_available()
            .map_err(|err| err.to_string())?;
        capabilities
            .ensure_required_skills_supported([])
            .map_err(|err| err.to_string())?;
        for plugin in capabilities.required_plugins() {
            validate_supported_plugin(plugin, capabilities.agent_key.as_str())?;
        }
        for item in capabilities.required_mcps() {
            if plugin_task_process_log_mcp(item) {
                validate_task_process_log_mcp_runtime(item)?;
            }
        }
        Ok(Self { capabilities })
    }

    pub(crate) fn policy_revision(&self) -> &str {
        self.capabilities.policy_revision.as_str()
    }

    pub(crate) fn agent_key(&self) -> &str {
        self.capabilities.agent_key.as_str()
    }

    pub(crate) fn selectable_builtin_kinds(&self) -> Vec<BuiltinMcpKind> {
        let mut out = self
            .capabilities
            .selectable_mcps()
            .filter_map(plugin_builtin_kind)
            .collect::<Vec<_>>();
        dedupe_builtin_kinds(&mut out);
        out
    }

    #[cfg(test)]
    pub(crate) fn selectable_builtin_kind_names(&self) -> Vec<String> {
        self.selectable_builtin_kinds()
            .into_iter()
            .map(|kind| kind.kind_name().to_string())
            .collect()
    }

    pub(crate) fn selectable_external_mcps(&self) -> Vec<&ResolvedMcp> {
        self.capabilities
            .selectable_mcps()
            .filter(|item| plugin_builtin_kind(item).is_none())
            .filter(|item| !plugin_task_process_log_mcp(item))
            .collect()
    }

    pub(crate) fn task_process_log_mcp_enabled(&self) -> bool {
        self.capabilities.mcps.iter().any(|item| {
            item.binding.enabled && item.resource.enabled && plugin_task_process_log_mcp(item)
        })
    }

    pub(crate) fn selectable_external_mcp_ids(&self) -> Vec<String> {
        self.selectable_external_mcps()
            .into_iter()
            .map(|item| item.resource.id.clone())
            .collect()
    }

    pub(crate) fn selectable_builtin_mcp_choices(&self) -> Vec<(String, String)> {
        let allowed = self
            .selectable_builtin_kinds()
            .into_iter()
            .chain(
                self.capabilities
                    .required_mcps()
                    .filter_map(plugin_builtin_kind),
            )
            .collect::<HashSet<_>>();
        self.capabilities
            .selectable_mcps()
            .filter_map(|item| {
                let kind = plugin_builtin_kind(item)?;
                if !complete_builtin_kind_dependencies([kind])
                    .into_iter()
                    .all(|dependency| allowed.contains(&dependency))
                {
                    return None;
                }
                let value = kind.kind_name().to_string();
                Some((value.clone(), mcp_choice_title(item, value.as_str())))
            })
            .collect::<BTreeMap<_, _>>()
            .into_iter()
            .collect()
    }

    pub(crate) fn selectable_external_mcp_choices(&self) -> Vec<(String, String)> {
        self.selectable_external_mcps()
            .into_iter()
            .map(|item| {
                (
                    item.resource.id.clone(),
                    mcp_choice_title(item, item.resource.id.as_str()),
                )
            })
            .collect()
    }

    pub(crate) fn selectable_plugins(&self) -> Vec<&ResolvedPlugin> {
        self.capabilities
            .selectable_plugins()
            .filter(|plugin| {
                validate_supported_plugin(plugin, self.capabilities.agent_key.as_str()).is_ok()
            })
            .collect()
    }

    pub(crate) fn validate_plugin_config(&self, config: &TaskPluginConfig) -> Result<(), String> {
        let mut seen_plugins = HashSet::new();
        for selected in &config.selected_plugins {
            let plugin_id = normalized_plugin_identifier(selected.plugin_id.as_str(), "plugin_id")?;
            if !seen_plugins.insert(plugin_id.clone()) {
                return Err(format!("Plugin is selected more than once: {plugin_id}"));
            }
            if !selected.selected_agent_ids.is_empty() {
                return Err(
                    "Task-level Plugin Agent selection is not supported; the Task Runner Agent is fixed by the MCP execution context"
                        .to_string(),
                );
            }
            let plugin = self
                .capabilities
                .plugins
                .iter()
                .find(|plugin| {
                    plugin.catalog.id == plugin_id
                        && (plugin.available
                            || plugin.status
                                == chatos_plugin_management_sdk::PluginAvailabilityStatus::PartiallyAvailable)
                })
                .ok_or_else(|| {
                    format!(
                        "Plugin is not selectable for {}: {plugin_id}",
                        self.capabilities.agent_key
                    )
                })?;
            if !plugin.binding.required
                && !self
                    .selectable_plugins()
                    .iter()
                    .any(|candidate| candidate.catalog.id == plugin_id)
            {
                return Err(format!(
                    "Plugin is not selectable for {}: {plugin_id}",
                    self.capabilities.agent_key
                ));
            }
            validate_plugin_component_selection(
                plugin,
                selected,
                self.capabilities.agent_key.as_str(),
            )?;
        }
        validate_command_invocations(config)?;
        Ok(())
    }

    pub(crate) fn validate_optional_config(&self, config: &TaskMcpConfig) -> Result<(), String> {
        let allowed_builtin = self
            .selectable_builtin_kinds()
            .into_iter()
            .chain(
                self.capabilities
                    .required_mcps()
                    .filter_map(plugin_builtin_kind),
            )
            .collect::<HashSet<_>>();
        let requested_builtin = config
            .enabled_builtin_kinds
            .iter()
            .map(|value| {
                builtin_kind_by_any(value)
                    .ok_or_else(|| format!("unknown builtin MCP kind: {value}"))
            })
            .collect::<Result<Vec<_>, _>>()?;
        for kind in complete_builtin_kind_dependencies(requested_builtin) {
            if !allowed_builtin.contains(&kind) {
                return Err(format!(
                    "builtin MCP is not selectable for {}: {}",
                    self.capabilities.agent_key,
                    kind.kind_name()
                ));
            }
        }

        let allowed_external = self
            .selectable_external_mcp_ids()
            .into_iter()
            .chain(
                self.capabilities
                    .required_mcps()
                    .filter(|item| {
                        plugin_builtin_kind(item).is_none() && !plugin_task_process_log_mcp(item)
                    })
                    .map(|item| item.resource.id.clone()),
            )
            .collect::<HashSet<_>>();
        for resource_id in &config.external_mcp_config_ids {
            if !allowed_external.contains(resource_id) {
                return Err(format!(
                    "external MCP is not selectable for {}: {resource_id}",
                    self.capabilities.agent_key
                ));
            }
        }
        Ok(())
    }

    pub(crate) fn effective_skills<'a>(
        &'a self,
        task: &TaskRecord,
    ) -> Result<Vec<&'a ResolvedSkill>, String> {
        if let Some(skill_id) = task.mcp_config.selected_skill_ids.first() {
            return Err(format!(
                "Local Connector Skill is unavailable in cloud Task Runner: {skill_id}"
            ));
        }
        Ok(Vec::new())
    }

    pub(crate) fn skill_snapshots(
        &self,
        task: &TaskRecord,
    ) -> Result<Vec<TaskSkillSnapshotView>, String> {
        self.effective_skills(task)?
            .into_iter()
            .map(|item| {
                let installation = item.installation.as_ref().ok_or_else(|| {
                    format!(
                        "Skill installation snapshot is missing: {}",
                        item.resource.id
                    )
                })?;
                Ok(TaskSkillSnapshotView {
                    skill_id: item.resource.id.clone(),
                    bundle_id: installation.bundle_id.clone(),
                    version: installation.version.clone(),
                    bundle_hash: installation.bundle_hash.clone(),
                    device_id: installation.device_id.clone(),
                    platform: installation.platform.clone(),
                    entrypoint_kind: item.resource.content.entrypoint_kind.clone(),
                })
            })
            .collect()
    }

    fn is_planning_agent(&self) -> bool {
        is_task_runner_planning_agent(self.capabilities.agent_key.as_str())
    }
}

fn mcp_choice_title(item: &ResolvedMcp, selection_value: &str) -> String {
    let display_name = item.resource.display_name.trim();
    let display_name = if display_name.is_empty() {
        item.resource.name.trim()
    } else {
        display_name
    };
    let tool_names = item
        .tool_snapshot
        .iter()
        .filter_map(|tool| tool.get("name").and_then(serde_json::Value::as_str))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .take(12)
        .collect::<Vec<_>>();
    let mut title = if display_name.is_empty() || display_name == selection_value {
        selection_value.to_string()
    } else {
        format!("{display_name} ({selection_value})")
    };
    if let Some(description) = item
        .resource
        .description
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        title.push_str(" - ");
        title.push_str(description);
    }
    if !tool_names.is_empty() {
        title.push_str(" [tools: ");
        title.push_str(tool_names.join(", ").as_str());
        title.push(']');
    }
    title
}

fn plugin_builtin_kind(item: &ResolvedMcp) -> Option<BuiltinMcpKind> {
    plugin_system_mcp_descriptor(item).and_then(|descriptor| descriptor.embedded_kind)
}

fn plugin_system_mcp_descriptor(item: &ResolvedMcp) -> Option<&'static SystemMcpDescriptor> {
    system_mcp_descriptor_for_record(&item.resource)
}

pub(super) fn plugin_task_process_log_mcp(item: &ResolvedMcp) -> bool {
    plugin_system_mcp_descriptor(item).is_some_and(|descriptor| {
        descriptor.key == chatos_plugin_management_sdk::SystemMcpKey::TaskProcessLog
    })
}

fn validate_task_process_log_mcp_runtime(item: &ResolvedMcp) -> Result<(), String> {
    let Some(descriptor) = plugin_system_mcp_descriptor(item) else {
        return Err(format!(
            "Task Process Log MCP has no system descriptor: {}",
            item.resource.id
        ));
    };
    if descriptor.backend != SystemMcpBackend::RunScopedBuiltin {
        return Err(format!(
            "Task Process Log MCP uses an unexpected backend: {:?}",
            descriptor.backend
        ));
    }
    Ok(())
}

fn is_task_runner_planning_agent(agent_key: &str) -> bool {
    parse_system_agent_key(agent_key).is_some_and(is_task_runner_planning_key)
}

fn is_task_runner_execution_agent(agent_key: &str) -> bool {
    parse_system_agent_key(agent_key).is_some_and(is_task_runner_execution_key)
}

fn normalized_plugin_identifier(value: &str, field: &str) -> Result<String, String> {
    let value = value.trim();
    if value.is_empty() {
        return Err(format!("{field} is required"));
    }
    Ok(value.to_string())
}

fn validate_command_invocations(config: &TaskPluginConfig) -> Result<(), String> {
    if config.command_invocations.len() > MAX_PLUGIN_COMMAND_INVOCATIONS {
        return Err(format!(
            "command_invocations must contain at most {MAX_PLUGIN_COMMAND_INVOCATIONS} items"
        ));
    }
    let mut seen = HashSet::new();
    for invocation in &config.command_invocations {
        let invocation = normalized_command_invocation(invocation)?;
        if !seen.insert((invocation.plugin_id.clone(), invocation.command_id.clone())) {
            return Err(format!(
                "Plugin Command invocation is duplicated: {}:{}",
                invocation.plugin_id, invocation.command_id
            ));
        }
        let selected = config
            .selected_plugins
            .iter()
            .find(|selected| selected.plugin_id.trim() == invocation.plugin_id)
            .ok_or_else(|| {
                format!(
                    "Plugin Command invocation references an unselected Plugin: {}:{}",
                    invocation.plugin_id, invocation.command_id
                )
            })?;
        if !selected
            .selected_command_ids
            .iter()
            .any(|command_id| command_id.trim() == invocation.command_id)
        {
            return Err(format!(
                "Plugin Command invocation references an unselected Command: {}:{}",
                invocation.plugin_id, invocation.command_id
            ));
        }
    }
    Ok(())
}

fn normalized_command_invocation(
    invocation: &PluginCommandInvocation,
) -> Result<PluginCommandInvocation, String> {
    Ok(PluginCommandInvocation {
        plugin_id: normalized_plugin_identifier(invocation.plugin_id.as_str(), "plugin_id")?,
        command_id: normalized_plugin_identifier(invocation.command_id.as_str(), "command_id")?,
        arguments: normalized_plugin_command_arguments(invocation.arguments.as_deref())?,
    })
}

fn normalized_plugin_command_arguments(value: Option<&str>) -> Result<Option<String>, String> {
    let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(None);
    };
    if value.len() > MAX_PLUGIN_COMMAND_ARGUMENT_BYTES {
        return Err(format!(
            "Plugin Command arguments exceed {MAX_PLUGIN_COMMAND_ARGUMENT_BYTES} bytes"
        ));
    }
    if value.contains('\0') {
        return Err("Plugin Command arguments contain NUL bytes".to_string());
    }
    Ok(Some(value.to_string()))
}

fn normalized_unique_ids(values: &[String], field: &str) -> Result<Vec<String>, String> {
    let mut seen = HashSet::new();
    let mut normalized = Vec::with_capacity(values.len());
    for value in values {
        let value = normalized_plugin_identifier(value, field)?;
        if !seen.insert(value.clone()) {
            return Err(format!("{field} contains a duplicate item: {value}"));
        }
        normalized.push(value);
    }
    normalized.sort();
    Ok(normalized)
}

fn dedupe_builtin_kinds(kinds: &mut Vec<BuiltinMcpKind>) {
    let mut seen = HashSet::new();
    kinds.retain(|kind| seen.insert(*kind));
}

#[cfg(test)]
#[path = "plugin_management_policy/tests.rs"]
mod tests;
