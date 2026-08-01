// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashSet;

use chatos_mcp::{system_mcp_descriptor_for_record, SystemMcpBackend, SystemMcpDescriptor};
use chatos_mcp_runtime::{builtin_kind_by_any, BuiltinMcpKind};
use chatos_plugin_management_sdk::{
    PluginCommandInvocation, PluginManagementClient, ResolveAgentCapabilitiesRequest,
    ResolvedAgentCapabilities, ResolvedMcp, ResolvedPlugin, ResolvedSkill, RunPluginSnapshot,
    SystemAgentKey, TaskPluginConfig,
};
use serde::Serialize;

use super::status_display::TaskScheduleModeExt;
use super::{RunService, TaskService};
use crate::auth::{get_current_access_token, CurrentUser};
use crate::models::{TaskMcpConfig, TaskRecord};

mod plugin_selection;
pub(crate) mod selectable_views;
mod task_config_application;

use plugin_selection::{
    plugin_selection_requires_local_execution, plugin_snapshot,
    validate_plugin_component_selection, validate_supported_plugin,
};

const MAX_PLUGIN_COMMAND_INVOCATIONS: usize = 64;
const MAX_PLUGIN_COMMAND_ARGUMENT_BYTES: usize = 16 * 1024;
#[cfg(test)]
const BUILTIN_RUNTIME_KIND: &str = chatos_plugin_management_sdk::LEGACY_BUILTIN_MCP_RUNTIME_KIND;

#[derive(Debug, Clone)]
pub(crate) struct TaskRunnerCapabilityPolicy {
    capabilities: ResolvedAgentCapabilities,
    portable_uses_local: bool,
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
    #[cfg(test)]
    fn new(capabilities: ResolvedAgentCapabilities) -> Result<Self, String> {
        Self::new_for_runtime(capabilities, false)
    }

    fn new_for_runtime(
        capabilities: ResolvedAgentCapabilities,
        portable_uses_local: bool,
    ) -> Result<Self, String> {
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
            validate_supported_plugin(
                plugin,
                capabilities.agent_key.as_str(),
                portable_uses_local,
            )?;
        }
        for item in capabilities.required_mcps() {
            if plugin_task_process_log_mcp(item) {
                validate_task_process_log_mcp_runtime(item)?;
            }
        }
        Ok(Self {
            capabilities,
            portable_uses_local,
        })
    }

    pub(crate) fn policy_revision(&self) -> &str {
        self.capabilities.policy_revision.as_str()
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

    pub(crate) fn selectable_plugins(&self) -> Vec<&ResolvedPlugin> {
        self.capabilities
            .selectable_plugins()
            .filter(|plugin| {
                validate_supported_plugin(
                    plugin,
                    self.capabilities.agent_key.as_str(),
                    self.portable_uses_local,
                )
                .is_ok()
            })
            .collect()
    }

    pub(crate) fn validate_plugin_config(&self, config: &TaskPluginConfig) -> Result<(), String> {
        if config.device_id.is_some() {
            normalized_plugin_identifier(
                config.device_id.as_deref().unwrap_or_default(),
                "device_id",
            )?;
        }
        if config.workspace_id.is_some() {
            normalized_optional_plugin_text(config.workspace_id.as_deref(), "workspace_id")?;
        }
        let mut seen_plugins = HashSet::new();
        let mut selected_agent_count = 0usize;
        for selected in &config.selected_plugins {
            let plugin_id = normalized_plugin_identifier(selected.plugin_id.as_str(), "plugin_id")?;
            if !seen_plugins.insert(plugin_id.clone()) {
                return Err(format!("Plugin is selected more than once: {plugin_id}"));
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
                self.portable_uses_local,
            )?;
            if plugin_selection_requires_local_execution(
                plugin,
                selected,
                self.capabilities.agent_key.as_str(),
                self.portable_uses_local,
            )? {
                normalized_plugin_identifier(
                    config.device_id.as_deref().unwrap_or_default(),
                    "device_id",
                )?;
            }
            selected_agent_count =
                selected_agent_count.saturating_add(selected.selected_agent_ids.len());
            if selected_agent_count > 1 {
                return Err("a Task may select at most one Plugin Agent".to_string());
            }
        }
        validate_command_invocations(config)?;
        Ok(())
    }

    pub(crate) fn validate_optional_config(&self, config: &TaskMcpConfig) -> Result<(), String> {
        let allowed_builtin = self
            .selectable_builtin_kinds()
            .into_iter()
            .collect::<HashSet<_>>();
        for value in &config.enabled_builtin_kinds {
            let kind = builtin_kind_by_any(value)
                .ok_or_else(|| format!("unknown builtin MCP kind: {value}"))?;
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

    pub(crate) fn plugin_snapshots(
        &self,
        task: &TaskRecord,
    ) -> Result<Vec<RunPluginSnapshot>, String> {
        self.validate_plugin_config(&task.plugin_config)?;
        let command_invocations = task
            .plugin_config
            .command_invocations
            .iter()
            .map(normalized_command_invocation)
            .collect::<Result<Vec<_>, _>>()?;
        let mut snapshots = task
            .plugin_config
            .selected_plugins
            .iter()
            .map(|selected| {
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
                plugin_snapshot(
                    plugin,
                    selected,
                    task.plugin_config.device_id.as_deref(),
                    task.plugin_config.workspace_id.as_deref(),
                    command_invocations.as_slice(),
                    self.capabilities.agent_key.as_str(),
                    self.portable_uses_local,
                )
            })
            .collect::<Result<Vec<_>, _>>()?;
        snapshots.sort_by(|left, right| left.plugin_id.cmp(&right.plugin_id));
        Ok(snapshots)
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

impl TaskService {
    pub(crate) async fn resolve_task_runner_policy_for_agent_on_device(
        &self,
        current_user: Option<&CurrentUser>,
        owner_user_id: Option<&str>,
        agent_key: SystemAgentKey,
        device_id: Option<String>,
    ) -> Result<Option<TaskRunnerCapabilityPolicy>, String> {
        let runtime_provider = device_id.as_ref().map(|_| "local_connector".to_string());
        self.resolve_task_runner_policy_for_agent_runtime(
            current_user,
            owner_user_id,
            agent_key,
            device_id,
            runtime_provider,
        )
        .await
    }

    pub(crate) async fn resolve_task_runner_policy_for_agent_runtime(
        &self,
        current_user: Option<&CurrentUser>,
        owner_user_id: Option<&str>,
        agent_key: SystemAgentKey,
        device_id: Option<String>,
        runtime_provider: Option<String>,
    ) -> Result<Option<TaskRunnerCapabilityPolicy>, String> {
        let Some(client) = self.plugin_management_client.as_ref() else {
            // Task definition CRUD does not execute an Agent or grant tools. The run path below
            // remains fail-closed and must resolve Plugin Management before model execution.
            return Ok(None);
        };
        let owner_user_id = resolved_owner_user_id(current_user, owner_user_id)?;
        resolve_policy(
            client,
            owner_user_id,
            get_current_access_token().as_deref(),
            agent_key,
            Some(TaskRunnerPolicyRuntimeContext {
                device_id,
                runtime_provider,
                ..TaskRunnerPolicyRuntimeContext::default()
            }),
        )
        .await
    }
}

impl RunService {
    pub(crate) async fn resolve_task_runner_policy_for_task(
        &self,
        task: &TaskRecord,
    ) -> Result<Option<TaskRunnerCapabilityPolicy>, String> {
        let Some(client) = self.plugin_management_client.as_ref() else {
            return Ok(None);
        };
        let owner_user_id = task_owner_user_id(task)
            .ok_or_else(|| "task owner user id is required for plugin policy".to_string())?;
        let project_source_type = self.task_project_source_type(task).await?;
        let runtime_provider = if project_source_type
            .as_deref()
            .is_some_and(|value| value.eq_ignore_ascii_case("local"))
        {
            "local_connector"
        } else {
            "cloud"
        };
        resolve_policy(
            client,
            owner_user_id,
            None,
            crate::models::task_runner_agent_key_for(
                task.task_profile.as_str(),
                task.mcp_config.requires_execution,
            ),
            Some(TaskRunnerPolicyRuntimeContext {
                task_profile: Some(task.task_profile.clone()),
                project_source_type,
                runtime_provider: Some(runtime_provider.to_string()),
                schedule_mode: Some(task.schedule.mode.mode_key().to_string()),
                device_id: normalized_text(task.plugin_config.device_id.clone()),
            }),
        )
        .await
    }

    async fn task_project_source_type(&self, task: &TaskRecord) -> Result<Option<String>, String> {
        if task.project_id == crate::models::PUBLIC_PROJECT_ID {
            return Ok(Some("public".to_string()));
        }
        let project_service =
            super::TaskProjectService::new_with_config(self.store.clone(), self.config.clone());
        Ok(project_service
            .get_project(task.project_id.as_str())
            .await?
            .and_then(|project| normalized_text(project.source_type)))
    }
}

#[derive(Debug, Clone, Default)]
struct TaskRunnerPolicyRuntimeContext {
    task_profile: Option<String>,
    project_source_type: Option<String>,
    runtime_provider: Option<String>,
    schedule_mode: Option<String>,
    device_id: Option<String>,
}

async fn resolve_policy(
    client: &PluginManagementClient,
    owner_user_id: &str,
    access_token: Option<&str>,
    agent_key: SystemAgentKey,
    runtime_context: Option<TaskRunnerPolicyRuntimeContext>,
) -> Result<Option<TaskRunnerCapabilityPolicy>, String> {
    let runtime_context = runtime_context.unwrap_or_default();
    let portable_uses_local =
        runtime_context.runtime_provider.as_deref() == Some("local_connector");
    let request = ResolveAgentCapabilitiesRequest::new(agent_key, owner_user_id)
        .with_runtime_context(
            runtime_context.task_profile,
            runtime_context.project_source_type,
            runtime_context.runtime_provider,
            runtime_context.schedule_mode,
        )
        .with_device_id(runtime_context.device_id);
    let capabilities = if let Some(access_token) = access_token {
        client
            .resolve_for_user(&request, access_token)
            .await
            .map_err(|err| err.to_string())?
    } else {
        client
            .resolve_for_service(&request)
            .await
            .map_err(|err| err.to_string())?
    };
    TaskRunnerCapabilityPolicy::new_for_runtime(capabilities, portable_uses_local).map(Some)
}

fn normalized_text(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn resolved_owner_user_id<'a>(
    current_user: Option<&'a CurrentUser>,
    task_owner_user_id: Option<&'a str>,
) -> Result<&'a str, String> {
    let current_owner = current_user.and_then(CurrentUser::effective_owner_user_id);
    let task_owner = task_owner_user_id
        .map(str::trim)
        .filter(|value| !value.is_empty());
    match (current_owner, task_owner) {
        (Some(current_owner), Some(task_owner)) if current_owner != task_owner => {
            Err("task owner does not match authenticated owner".to_string())
        }
        (Some(owner), _) | (_, Some(owner)) => Ok(owner),
        (None, None) => Err("task owner user id is required for plugin policy".to_string()),
    }
}

fn task_owner_user_id(task: &TaskRecord) -> Option<&str> {
    task.owner_user_id
        .as_deref()
        .or(task.creator_user_id.as_deref())
        .or(Some(task.subject_id.as_str()))
        .map(str::trim)
        .filter(|value| !value.is_empty())
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
    agent_key == SystemAgentKey::TaskRunnerPlanPhase.as_str()
}

fn is_task_runner_execution_agent(agent_key: &str) -> bool {
    agent_key == SystemAgentKey::TaskRunnerRunPhase.as_str()
}

fn normalized_plugin_identifier(value: &str, field: &str) -> Result<String, String> {
    let value = value.trim();
    if value.is_empty() {
        return Err(format!("{field} is required"));
    }
    Ok(value.to_string())
}

fn normalized_optional_plugin_text(
    value: Option<&str>,
    field: &str,
) -> Result<Option<String>, String> {
    match value {
        Some(value) => Ok(Some(normalized_plugin_identifier(value, field)?)),
        None => Ok(None),
    }
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
mod tests;
