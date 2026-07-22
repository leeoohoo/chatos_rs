// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashSet;

use chatos_mcp::{
    system_mcp_descriptor_for_record, SystemMcpBackend, SystemMcpDescriptor, SystemMcpHost,
};
use chatos_mcp_runtime::{builtin_kind_by_any, BuiltinMcpKind};
use chatos_plugin_management_sdk::{
    McpRecord as PluginMcpRecord, PluginManagementClient, ResolveAgentCapabilitiesRequest,
    ResolvedAgentCapabilities, ResolvedMcp, ResolvedSkill, SystemAgentKey,
};
use serde::Serialize;

use super::status_display::TaskScheduleModeExt;
use super::{RunService, TaskService};
use crate::auth::{get_current_access_token, CurrentUser};
use crate::models::{TaskMcpConfig, TaskRecord};

const LOCAL_CONNECTOR_DISCOVERED_SOURCE_KIND: &str = "local_connector_discovered";
const CLOUD_EXTERNAL_RUNTIME_KINDS: [&str; 2] = ["http", "stdio_cloud"];
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
pub(crate) struct SelectableSkillView {
    pub id: String,
    pub name: String,
    pub display_name: String,
    pub description: Option<String>,
    pub bundle_id: Option<String>,
    pub version: Option<String>,
    pub bundle_hash: Option<String>,
    pub entrypoint_kind: Option<String>,
    pub device_id: Option<String>,
    pub platform: Option<String>,
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
        let planning_agent = capabilities.agent_key == SystemAgentKey::TaskRunnerPlanPhase.as_str();
        capabilities
            .ensure_required_available()
            .map_err(|err| err.to_string())?;
        capabilities
            .ensure_required_skills_supported(std::iter::empty::<&str>())
            .map_err(|err| err.to_string())?;
        for item in capabilities.required_mcps() {
            if let Some(kind) = plugin_builtin_kind(item) {
                if planning_agent && !planning_builtin_kind_allowed(kind) {
                    return Err(format!(
                        "mutating builtin MCP cannot be required for task_runner_plan_phase: {}",
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

    pub(crate) fn selectable_skills(&self) -> Vec<&ResolvedSkill> {
        Vec::new()
    }

    pub(crate) fn selectable_skill_views(&self) -> Vec<SelectableSkillView> {
        self.selectable_skills()
            .into_iter()
            .map(|item| SelectableSkillView {
                id: item.resource.id.clone(),
                name: item.resource.name.clone(),
                display_name: item.resource.display_name.clone(),
                description: item.resource.description.clone(),
                bundle_id: item.resource.content.bundle_id.clone(),
                version: item.resource.content.bundle_version.clone(),
                bundle_hash: item.resource.content.bundle_hash.clone(),
                entrypoint_kind: item.resource.content.entrypoint_kind.clone(),
                device_id: item
                    .installation
                    .as_ref()
                    .map(|value| value.device_id.clone()),
                platform: item
                    .installation
                    .as_ref()
                    .map(|value| value.platform.clone()),
            })
            .collect()
    }

    pub(crate) fn selectable_skill_ids(&self) -> Vec<String> {
        self.selectable_skills()
            .into_iter()
            .map(|item| item.resource.id.clone())
            .collect()
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
        let allowed_skills = self
            .selectable_skill_ids()
            .into_iter()
            .collect::<HashSet<_>>();
        for skill_id in &config.selected_skill_ids {
            if !allowed_skills.contains(skill_id) {
                return Err(format!(
                    "Skill is not selectable for {}: {skill_id}",
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
        let allowed_optional_skills = self
            .selectable_skill_ids()
            .into_iter()
            .collect::<HashSet<_>>();
        let mut effective_skills = task
            .mcp_config
            .selected_skill_ids
            .iter()
            .filter(|resource_id| allowed_optional_skills.contains(resource_id.as_str()))
            .cloned()
            .collect::<Vec<_>>();
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
        self.capabilities.agent_key == SystemAgentKey::TaskRunnerPlanPhase.as_str()
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
            None,
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
        );
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

fn dedupe_builtin_kinds(kinds: &mut Vec<BuiltinMcpKind>) {
    let mut seen = HashSet::new();
    kinds.retain(|kind| seen.insert(*kind));
}

#[cfg(test)]
mod tests;
