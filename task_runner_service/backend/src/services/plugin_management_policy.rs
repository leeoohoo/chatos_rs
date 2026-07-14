// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashSet;

use chatos_mcp_runtime::{builtin_kind_by_any, BuiltinMcpKind};
use chatos_plugin_management_sdk::{
    McpRecord as PluginMcpRecord, PluginManagementClient, ResolveAgentCapabilitiesRequest,
    ResolvedAgentCapabilities, ResolvedMcp, ResolvedSkill, SystemAgentKey,
};
use serde::Serialize;

use super::{RunService, TaskService};
use crate::auth::{get_current_access_token, CurrentUser};
use crate::models::{TaskMcpConfig, TaskRecord};

const BUILTIN_RUNTIME_KIND: &str = "builtin";
const LOCAL_CONNECTOR_DISCOVERED_SOURCE_KIND: &str = "local_connector_discovered";
const USER_CREATED_SOURCE_KIND: &str = "user_created";
const LOCAL_CONNECTOR_USER_RUNTIME_KINDS: [&str; 2] =
    ["local_connector_stdio", "local_connector_http"];

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
        capabilities
            .ensure_required_available()
            .map_err(|err| err.to_string())?;
        let supported_skill_ids = capabilities
            .skills
            .iter()
            .filter(|item| item.resource.content.kind == "local_connector_bundle")
            .map(|item| item.resource.id.as_str())
            .collect::<Vec<_>>();
        capabilities
            .ensure_required_skills_supported(supported_skill_ids)
            .map_err(|err| err.to_string())?;
        for item in capabilities.required_mcps() {
            validate_local_connector_user_runtime(item)?;
        }
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
            .filter(|item| validate_local_connector_user_runtime(item).is_ok())
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
        self.capabilities.selectable_skills().collect()
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
                    "builtin MCP is not selectable for task_runner_run_phase: {}",
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
                    "external MCP is not selectable for task_runner_run_phase: {resource_id}"
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
                    "Skill is not selectable for task_runner_run_phase: {skill_id}"
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
        let mut out = Vec::new();
        for skill_id in &task.mcp_config.selected_skill_ids {
            let item = self
                .capabilities
                .skills
                .iter()
                .find(|item| item.resource.id == *skill_id && item.available)
                .ok_or_else(|| format!("effective Skill is unavailable: {skill_id}"))?;
            out.push(item);
        }
        Ok(out)
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
}

impl TaskService {
    pub(crate) async fn resolve_task_runner_policy(
        &self,
        current_user: Option<&CurrentUser>,
        owner_user_id: Option<&str>,
    ) -> Result<Option<TaskRunnerCapabilityPolicy>, String> {
        let Some(client) = self.plugin_management_client.as_ref() else {
            return Ok(None);
        };
        let owner_user_id = resolved_owner_user_id(current_user, owner_user_id)?;
        resolve_policy(client, owner_user_id, get_current_access_token().as_deref()).await
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
        resolve_policy(client, owner_user_id, None).await
    }
}

async fn resolve_policy(
    client: &PluginManagementClient,
    owner_user_id: &str,
    access_token: Option<&str>,
) -> Result<Option<TaskRunnerCapabilityPolicy>, String> {
    let request =
        ResolveAgentCapabilitiesRequest::new(SystemAgentKey::TaskRunnerRunPhase, owner_user_id);
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
    if item.resource.runtime.kind != BUILTIN_RUNTIME_KIND {
        return None;
    }
    item.resource
        .runtime
        .builtin_kind
        .as_deref()
        .and_then(builtin_kind_by_any)
}

fn validate_local_connector_user_runtime(item: &ResolvedMcp) -> Result<(), String> {
    if !matches!(
        item.resource.source_kind.as_str(),
        LOCAL_CONNECTOR_DISCOVERED_SOURCE_KIND | USER_CREATED_SOURCE_KIND
    ) {
        return Ok(());
    }
    if !LOCAL_CONNECTOR_USER_RUNTIME_KINDS.contains(&item.resource.runtime.kind.as_str()) {
        return Err(format!(
            "local connector user MCP {} has invalid runtime kind: {}",
            item.resource.id, item.resource.runtime.kind
        ));
    }
    let local = item
        .resource
        .runtime
        .local_connector
        .as_ref()
        .ok_or_else(|| {
            format!(
                "local connector user MCP {} is missing runtime reference",
                item.resource.id
            )
        })?;
    for (field, value) in [
        ("device_id", local.device_id.as_deref()),
        ("manifest_id", local.manifest_id.as_deref()),
    ] {
        if value
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .is_none()
        {
            return Err(format!(
                "local connector user MCP {} is missing {field}",
                item.resource.id
            ));
        }
    }
    Ok(())
}

fn dedupe_builtin_kinds(kinds: &mut Vec<BuiltinMcpKind>) {
    let mut seen = HashSet::new();
    kinds.retain(|kind| seen.insert(*kind));
}

#[cfg(test)]
mod tests;
