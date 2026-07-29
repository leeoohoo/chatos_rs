// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::{BTreeMap, HashMap, HashSet};

use chatos_mcp::{
    system_mcp_descriptor_for_record, SystemMcpBackend, SystemMcpDescriptor, SystemMcpHost,
};
use chatos_mcp_runtime::{builtin_kind_by_any, BuiltinMcpKind};
use chatos_plugin_management_sdk::{
    McpRecord as PluginMcpRecord, PluginCommandInvocation, PluginComponentKind,
    PluginManagementClient, ResolveAgentCapabilitiesRequest, ResolvedAgentCapabilities,
    ResolvedMcp, ResolvedPlugin, ResolvedSkill, RunPluginComponentSnapshot, RunPluginSnapshot,
    SelectedPluginRef, SystemAgentKey, TaskPluginConfig,
};
use serde::Serialize;
use serde_json::Value;

use super::status_display::TaskScheduleModeExt;
use super::{RunService, TaskService};
use crate::auth::{get_current_access_token, CurrentUser};
use crate::models::{TaskMcpConfig, TaskRecord};

const LOCAL_CONNECTOR_DISCOVERED_SOURCE_KIND: &str = "local_connector_discovered";
const CLOUD_EXTERNAL_RUNTIME_KINDS: [&str; 2] = ["http", "stdio_cloud"];
const MAX_PLUGIN_COMMAND_INVOCATIONS: usize = 64;
const MAX_PLUGIN_COMMAND_ARGUMENT_BYTES: usize = 16 * 1024;
#[cfg(test)]
const BUILTIN_RUNTIME_KIND: &str = chatos_plugin_management_sdk::LEGACY_BUILTIN_MCP_RUNTIME_KIND;

#[derive(Debug, Clone)]
pub(crate) struct TaskRunnerCapabilityPolicy {
    capabilities: ResolvedAgentCapabilities,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct SelectableExternalMcpView {
    pub id: String,
    pub name: String,
    pub display_name: String,
    pub description: Option<String>,
    pub runtime_kind: String,
    pub visibility: String,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct SelectablePluginView {
    pub id: String,
    pub plugin_key: String,
    pub display_name: String,
    pub description: String,
    pub version: String,
    pub release_id: String,
    pub artifact_sha256: String,
    pub device_id: String,
    pub component_keys: Vec<String>,
    pub commands: Vec<SelectablePluginCommandView>,
    pub agents: Vec<SelectablePluginAgentView>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct SelectablePluginCommandView {
    pub command_id: String,
    pub display_name: String,
    pub description: Option<String>,
    pub argument_hint: Option<String>,
    pub requires_confirmation: bool,
    pub target_agent: Option<String>,
    pub allowed_tools: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct SelectablePluginAgentView {
    pub agent_id: String,
    pub display_name: String,
    pub description: Option<String>,
    pub base_agent: String,
    pub allowed_tools: Vec<String>,
    pub max_iterations: usize,
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
        let planning_agent = is_task_runner_planning_agent(capabilities.agent_key.as_str());
        let cloud_execution_agent =
            capabilities.agent_key == SystemAgentKey::TaskRunnerRunPhase.as_str();
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
            if let Some(kind) = plugin_builtin_kind(item) {
                if planning_agent && !planning_builtin_kind_allowed(kind) {
                    return Err(format!(
                        "mutating builtin MCP cannot be required for task_runner_plan_phase: {}",
                        kind.kind_name()
                    ));
                }
                if cloud_execution_agent && !cloud_execution_builtin_kind_allowed(kind) {
                    return Err(format!(
                        "builtin MCP cannot be required for cloud task_runner_run_phase: {}",
                        kind.kind_name()
                    ));
                }
            } else {
                validate_cloud_external_mcp_runtime(item)?;
                if planning_agent && item.resource.security.allow_writes != Some(false) {
                    return Err(format!(
                        "external MCP required by task_runner_plan_phase must explicitly disallow writes: {}",
                        item.resource.id
                    ));
                }
            }
        }
        validate_configured_builtin_dependencies(&capabilities)?;
        Ok(Self { capabilities })
    }

    pub(crate) fn policy_revision(&self) -> &str {
        self.capabilities.policy_revision.as_str()
    }

    pub(crate) fn selectable_builtin_kinds(&self) -> Vec<BuiltinMcpKind> {
        let mut out = self
            .capabilities
            .selectable_mcps()
            .filter_map(plugin_builtin_kind)
            .filter(|kind| !self.is_planning_agent() || planning_builtin_kind_allowed(*kind))
            .filter(|kind| {
                !self.is_cloud_execution_agent() || cloud_execution_builtin_kind_allowed(*kind)
            })
            .collect::<Vec<_>>();
        dedupe_builtin_kinds(&mut out);
        out
    }

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
            .filter(|item| {
                !self.is_planning_agent() || item.resource.security.allow_writes == Some(false)
            })
            .filter(|item| validate_cloud_external_mcp_runtime(item).is_ok())
            .collect()
    }

    pub(crate) fn selectable_external_mcp_views(&self) -> Vec<SelectableExternalMcpView> {
        self.selectable_external_mcps()
            .into_iter()
            .map(|item| SelectableExternalMcpView {
                id: item.resource.id.clone(),
                name: item.resource.name.clone(),
                display_name: item.resource.display_name.clone(),
                description: item.resource.description.clone(),
                runtime_kind: item.resource.runtime.kind.clone(),
                visibility: item.resource.visibility.clone(),
            })
            .collect()
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
                validate_supported_plugin(plugin, self.capabilities.agent_key.as_str()).is_ok()
            })
            .collect()
    }

    pub(crate) fn selectable_plugin_views(&self) -> Vec<SelectablePluginView> {
        self.selectable_plugins()
            .into_iter()
            .filter_map(|plugin| {
                let release = plugin.release.as_ref()?;
                let installation = plugin.installation.as_ref()?;
                Some(SelectablePluginView {
                    id: plugin.catalog.id.clone(),
                    plugin_key: plugin.catalog.plugin_key.clone(),
                    display_name: plugin.catalog.display_name.clone(),
                    description: plugin.catalog.description.clone(),
                    version: release.version.clone(),
                    release_id: release.id.clone(),
                    artifact_sha256: release.artifact_sha256.clone(),
                    device_id: installation.device_id.clone(),
                    component_keys: plugin
                        .components
                        .iter()
                        .filter(|component| component.available)
                        .map(|component| component.component.component_key.clone())
                        .collect(),
                    commands: plugin
                        .components
                        .iter()
                        .filter(|component| {
                            component.available
                                && component.component.kind == PluginComponentKind::Command
                                && (component
                                    .component
                                    .metadata
                                    .get("target_agent")
                                    .and_then(Value::as_str)
                                    .is_some_and(|target_agent| {
                                        target_agent == self.capabilities.agent_key
                                    })
                                    || (is_task_runner_execution_agent(
                                        self.capabilities.agent_key.as_str(),
                                    ) && !component
                                        .component
                                        .metadata
                                        .contains_key("target_agent")))
                        })
                        .map(|component| SelectablePluginCommandView {
                            command_id: component.component.component_key.clone(),
                            display_name: component.component.display_name.clone(),
                            description: component
                                .component
                                .metadata
                                .get("description")
                                .and_then(Value::as_str)
                                .map(str::to_string),
                            argument_hint: component
                                .component
                                .metadata
                                .get("argument_hint")
                                .and_then(Value::as_str)
                                .map(str::to_string),
                            requires_confirmation: component
                                .component
                                .metadata
                                .get("requires_confirmation")
                                .and_then(Value::as_bool)
                                .unwrap_or(false),
                            target_agent: component
                                .component
                                .metadata
                                .get("target_agent")
                                .and_then(Value::as_str)
                                .map(str::to_string),
                            allowed_tools: component
                                .component
                                .metadata
                                .get("allowed_tools")
                                .and_then(Value::as_array)
                                .into_iter()
                                .flatten()
                                .filter_map(Value::as_str)
                                .map(str::to_string)
                                .collect(),
                        })
                        .collect(),
                    agents: plugin
                        .components
                        .iter()
                        .filter(|component| {
                            component.available
                                && component.component.kind == PluginComponentKind::Agent
                                && component
                                    .component
                                    .metadata
                                    .get("base_agent")
                                    .and_then(Value::as_str)
                                    == Some(self.capabilities.agent_key.as_str())
                        })
                        .filter_map(|component| {
                            Some(SelectablePluginAgentView {
                                agent_id: component.component.component_key.clone(),
                                display_name: component.component.display_name.clone(),
                                description: component
                                    .component
                                    .metadata
                                    .get("description")
                                    .and_then(Value::as_str)
                                    .map(str::to_string),
                                base_agent: component
                                    .component
                                    .metadata
                                    .get("base_agent")
                                    .and_then(Value::as_str)?
                                    .to_string(),
                                allowed_tools: component
                                    .component
                                    .metadata
                                    .get("allowed_tools")
                                    .and_then(Value::as_array)
                                    .into_iter()
                                    .flatten()
                                    .filter_map(Value::as_str)
                                    .map(str::to_string)
                                    .collect(),
                                max_iterations: component
                                    .component
                                    .metadata
                                    .get("max_iterations")
                                    .and_then(Value::as_u64)
                                    .and_then(|value| usize::try_from(value).ok())?,
                            })
                        })
                        .collect(),
                })
            })
            .collect()
    }

    pub(crate) fn validate_plugin_config(&self, config: &TaskPluginConfig) -> Result<(), String> {
        if !config.selected_plugins.is_empty() {
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
                .find(|plugin| plugin.catalog.id == plugin_id && plugin.available)
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

    pub(crate) fn apply_to_task(&self, task: &mut TaskRecord) -> Result<(), String> {
        self.capabilities
            .ensure_required_available()
            .map_err(|err| err.to_string())?;
        task.mcp_config.enabled = true;

        let allowed_optional_builtin = self
            .selectable_builtin_kinds()
            .into_iter()
            .collect::<HashSet<_>>();
        let mut effective_builtin = task
            .mcp_config
            .enabled_builtin_kinds
            .iter()
            .filter_map(|value| builtin_kind_by_any(value))
            .filter(|kind| allowed_optional_builtin.contains(kind))
            .collect::<Vec<_>>();
        effective_builtin.extend(
            self.capabilities
                .required_mcps()
                .filter(|item| item.available)
                .filter_map(plugin_builtin_kind),
        );
        if self.is_planning_agent() {
            effective_builtin.extend(self.selectable_builtin_kinds());
        }
        dedupe_builtin_kinds(&mut effective_builtin);
        task.mcp_config.enabled_builtin_kinds = effective_builtin
            .into_iter()
            .map(|kind| kind.kind_name().to_string())
            .collect();

        let allowed_optional_external = self
            .selectable_external_mcp_ids()
            .into_iter()
            .collect::<HashSet<_>>();
        let mut effective_external = task
            .mcp_config
            .external_mcp_config_ids
            .iter()
            .filter(|resource_id| allowed_optional_external.contains(resource_id.as_str()))
            .cloned()
            .collect::<Vec<_>>();
        effective_external.extend(
            self.capabilities
                .required_mcps()
                .filter(|item| item.available && plugin_builtin_kind(item).is_none())
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
        self.apply_plugins_to_task(task)?;
        Ok(())
    }

    pub(crate) fn apply_plugins_to_task(&self, task: &mut TaskRecord) -> Result<(), String> {
        self.validate_plugin_config(&task.plugin_config)?;
        let allowed_effective = self
            .selectable_plugins()
            .into_iter()
            .map(|plugin| plugin.catalog.id.as_str())
            .chain(
                self.capabilities
                    .required_plugins()
                    .filter(|plugin| plugin.available)
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
                    selected_agent_ids: normalized_unique_ids(
                        &selected.selected_agent_ids,
                        "selected_agent_ids",
                    )?,
                });
            }
        }
        for plugin in self
            .capabilities
            .required_plugins()
            .filter(|plugin| plugin.available)
        {
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
        task.plugin_config.device_id =
            normalized_optional_plugin_text(task.plugin_config.device_id.as_deref(), "device_id")?;
        task.plugin_config.workspace_id = normalized_optional_plugin_text(
            task.plugin_config.workspace_id.as_deref(),
            "workspace_id",
        )?;
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
                .find(|plugin| plugin.catalog.id == selected.plugin_id && plugin.available)
                .ok_or_else(|| {
                    format!("effective Plugin is unavailable: {}", selected.plugin_id)
                })?;
            if plugin.catalog.name == "browser" {
                dependencies.push(BuiltinMcpKind::BrowserTools);
            }
        }
        for dependency in dependencies {
            let available = self
                .capabilities
                .mcps
                .iter()
                .any(|item| item.available && plugin_builtin_kind(item) == Some(dependency));
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
                    .find(|plugin| plugin.catalog.id == selected.plugin_id && plugin.available)
                    .ok_or_else(|| {
                        format!("effective Plugin is unavailable: {}", selected.plugin_id)
                    })?;
                plugin_snapshot(
                    plugin,
                    selected,
                    task.plugin_config.workspace_id.as_deref(),
                    command_invocations.as_slice(),
                    self.capabilities.agent_key.as_str(),
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

    pub(crate) fn effective_external_mcps<'a>(
        &'a self,
        task: &TaskRecord,
    ) -> Result<Vec<&'a PluginMcpRecord>, String> {
        let mut out = Vec::new();
        for resource_id in &task.mcp_config.external_mcp_config_ids {
            let item = self
                .capabilities
                .mcps
                .iter()
                .find(|item| item.resource.id == *resource_id && item.available)
                .ok_or_else(|| format!("effective MCP resource is unavailable: {resource_id}"))?;
            if plugin_builtin_kind(item).is_none() {
                validate_cloud_external_mcp_runtime(item)?;
                out.push(&item.resource);
            }
        }
        Ok(out)
    }

    pub(crate) fn compose_provider_skills_prompt<'a>(
        &self,
        effective_mcp_identifiers: impl IntoIterator<Item = &'a str>,
        locale: &str,
    ) -> Option<String> {
        self.capabilities
            .compose_provider_skills_prompt(effective_mcp_identifiers, Some(locale))
    }

    fn is_planning_agent(&self) -> bool {
        is_task_runner_planning_agent(self.capabilities.agent_key.as_str())
    }

    fn is_cloud_execution_agent(&self) -> bool {
        self.capabilities.agent_key == SystemAgentKey::TaskRunnerRunPhase.as_str()
    }
}

impl TaskService {
    pub(crate) async fn resolve_task_runner_policy(
        &self,
        current_user: Option<&CurrentUser>,
        owner_user_id: Option<&str>,
    ) -> Result<Option<TaskRunnerCapabilityPolicy>, String> {
        self.resolve_task_runner_policy_for_agent(
            current_user,
            owner_user_id,
            SystemAgentKey::TaskRunnerRunPhase,
        )
        .await
    }

    pub(crate) async fn resolve_task_runner_policy_for_agent(
        &self,
        current_user: Option<&CurrentUser>,
        owner_user_id: Option<&str>,
        agent_key: SystemAgentKey,
    ) -> Result<Option<TaskRunnerCapabilityPolicy>, String> {
        self.resolve_task_runner_policy_for_agent_on_device(
            current_user,
            owner_user_id,
            agent_key,
            None,
        )
        .await
    }

    pub(crate) async fn resolve_task_runner_policy_for_agent_on_device(
        &self,
        current_user: Option<&CurrentUser>,
        owner_user_id: Option<&str>,
        agent_key: SystemAgentKey,
        device_id: Option<String>,
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
                runtime_provider: Some("cloud".to_string()),
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
    TaskRunnerCapabilityPolicy::new(capabilities).map(Some)
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

fn planning_builtin_kind_allowed(kind: BuiltinMcpKind) -> bool {
    !matches!(
        kind,
        BuiltinMcpKind::CodeMaintainerWrite
            | BuiltinMcpKind::TerminalController
            | BuiltinMcpKind::RemoteConnectionController
    )
}

fn cloud_execution_builtin_kind_allowed(kind: BuiltinMcpKind) -> bool {
    !matches!(kind, BuiltinMcpKind::RemoteConnectionController)
}

fn is_task_runner_planning_agent(agent_key: &str) -> bool {
    agent_key == SystemAgentKey::TaskRunnerPlanPhase.as_str()
        || agent_key == SystemAgentKey::TaskRunnerLocalPlanPhase.as_str()
}

fn is_task_runner_execution_agent(agent_key: &str) -> bool {
    agent_key == SystemAgentKey::TaskRunnerRunPhase.as_str()
        || agent_key == SystemAgentKey::TaskRunnerLocalRunPhase.as_str()
}

fn validate_configured_builtin_dependencies(
    capabilities: &ResolvedAgentCapabilities,
) -> Result<(), String> {
    let configured = capabilities
        .mcps
        .iter()
        .filter(|item| item.available && item.binding.enabled && item.resource.enabled)
        .filter_map(plugin_builtin_kind)
        .collect::<HashSet<_>>();
    if configured.contains(&BuiltinMcpKind::CodeMaintainerWrite)
        && !configured.contains(&BuiltinMcpKind::CodeMaintainerRead)
    {
        return Err(format!(
            "Plugin Management config for {} enables CodeMaintainerWrite without CodeMaintainerRead",
            capabilities.agent_key
        ));
    }
    Ok(())
}

fn validate_cloud_external_mcp_runtime(item: &ResolvedMcp) -> Result<(), String> {
    let runtime_kind = item.resource.runtime.kind.as_str();
    if item.resource.source_kind == LOCAL_CONNECTOR_DISCOVERED_SOURCE_KIND
        || runtime_kind.starts_with("local_connector_")
        || item.resource.runtime.local_connector.is_some()
    {
        return Err(format!(
            "Local Connector MCP is unavailable in cloud Task Runner: {}",
            item.resource.id
        ));
    }
    if let Some(descriptor) = plugin_system_mcp_descriptor(item) {
        if descriptor.embedded_kind.is_some() {
            return Err(format!(
                "embedded system MCP cannot be loaded as an external MCP: {}",
                descriptor.server_name
            ));
        }
        if !descriptor.supports_host(SystemMcpHost::TaskRunner)
            || !matches!(
                descriptor.backend,
                SystemMcpBackend::ServiceHttp | SystemMcpBackend::ServiceDynamic
            )
        {
            return Err(format!(
                "system MCP {} has no Task Runner service backend",
                descriptor.server_name
            ));
        }
        return Ok(());
    }
    if !CLOUD_EXTERNAL_RUNTIME_KINDS.contains(&runtime_kind) {
        return Err(format!(
            "cloud Task Runner does not support MCP runtime kind {} for {}",
            runtime_kind, item.resource.id
        ));
    }
    Ok(())
}

fn validate_supported_plugin(plugin: &ResolvedPlugin, expected_agent: &str) -> Result<(), String> {
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

fn validate_plugin_component_selection(
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

fn plugin_snapshot(
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
    component: &chatos_plugin_management_sdk::PluginComponentDescriptor,
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

fn plugin_skill_id(component: &chatos_plugin_management_sdk::PluginComponentDescriptor) -> String {
    component
        .metadata
        .get("skill_id")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(component.component_key.as_str())
        .to_string()
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

#[cfg(test)]
mod tests;
