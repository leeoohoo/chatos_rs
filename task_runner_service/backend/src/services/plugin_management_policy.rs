// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashSet;

use chatos_mcp_runtime::{builtin_kind_by_any, BuiltinMcpKind};
use chatos_plugin_management_sdk::{
    McpRecord as PluginMcpRecord, PluginManagementClient, ResolveAgentCapabilitiesRequest,
    ResolvedAgentCapabilities, ResolvedMcp, SystemAgentKey,
};
use serde::Serialize;

use super::{RunService, TaskService};
use crate::auth::{get_current_access_token, CurrentUser};
use crate::models::{TaskMcpConfig, TaskRecord};

const BUILTIN_RUNTIME_KIND: &str = "builtin";
const LOCAL_CONNECTOR_DISCOVERED_SOURCE_KIND: &str = "local_connector_discovered";
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

impl TaskRunnerCapabilityPolicy {
    fn new(capabilities: ResolvedAgentCapabilities) -> Result<Self, String> {
        capabilities
            .ensure_required_available()
            .map_err(|err| err.to_string())?;
        capabilities
            .ensure_required_skills_supported(std::iter::empty::<&str>())
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
        Ok(())
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
    if item.resource.source_kind != LOCAL_CONNECTOR_DISCOVERED_SOURCE_KIND {
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
mod tests {
    use super::*;
    use crate::models::{
        now_rfc3339, TaskMcpConfig, TaskRecord, TaskScheduleConfig, TaskStatus, TaskToolState,
    };
    use chatos_plugin_management_sdk::{
        AgentBindingRecord, BindingConditions, LocalConnectorRef, McpRuntime, ResourceMetadata,
        ResourceSecurity,
    };

    fn resolved_mcp(
        id: &str,
        runtime_kind: &str,
        builtin_kind: Option<&str>,
        required: bool,
        available: bool,
    ) -> ResolvedMcp {
        ResolvedMcp {
            resource: PluginMcpRecord {
                id: id.to_string(),
                owner_user_id: "owner-1".to_string(),
                owner_kind: "system".to_string(),
                visibility: "system_private".to_string(),
                source_kind: "system_seed".to_string(),
                name: id.to_string(),
                display_name: id.to_string(),
                description: None,
                enabled: true,
                runtime: McpRuntime {
                    kind: runtime_kind.to_string(),
                    builtin_kind: builtin_kind.map(ToOwned::to_owned),
                    url: (runtime_kind == "http").then(|| "http://127.0.0.1/mcp".to_string()),
                    ..McpRuntime::default()
                },
                security: ResourceSecurity::default(),
                metadata: ResourceMetadata::default(),
                created_by: "system".to_string(),
                updated_by: "system".to_string(),
                created_at: "now".to_string(),
                updated_at: "now".to_string(),
            },
            binding: AgentBindingRecord {
                id: format!("binding-{id}"),
                agent_key: SystemAgentKey::TaskRunnerRunPhase.as_str().to_string(),
                binding_scope: if required {
                    "system_required".to_string()
                } else {
                    "global_default".to_string()
                },
                owner_user_id: None,
                resource_kind: "mcp".to_string(),
                resource_id: id.to_string(),
                enabled: true,
                required,
                priority: 0,
                conditions: BindingConditions::default(),
                created_by: "system".to_string(),
                updated_by: "system".to_string(),
                created_at: "now".to_string(),
                updated_at: "now".to_string(),
            },
            available,
            status: if available { "available" } else { "offline" }.to_string(),
            reason: (!available).then(|| "offline".to_string()),
        }
    }

    fn policy() -> TaskRunnerCapabilityPolicy {
        TaskRunnerCapabilityPolicy::new(ResolvedAgentCapabilities {
            agent_key: SystemAgentKey::TaskRunnerRunPhase.as_str().to_string(),
            owner_user_id: "owner-1".to_string(),
            policy_revision: "revision-1".to_string(),
            generated_at: "now".to_string(),
            agent_enabled: true,
            mcps: vec![
                resolved_mcp(
                    "task-manager",
                    BUILTIN_RUNTIME_KIND,
                    Some("TaskManager"),
                    true,
                    true,
                ),
                resolved_mcp(
                    "ask-user",
                    BUILTIN_RUNTIME_KIND,
                    Some("AskUser"),
                    true,
                    true,
                ),
                resolved_mcp(
                    "read",
                    BUILTIN_RUNTIME_KIND,
                    Some("CodeMaintainerRead"),
                    false,
                    true,
                ),
                resolved_mcp(
                    "write",
                    BUILTIN_RUNTIME_KIND,
                    Some("CodeMaintainerWrite"),
                    false,
                    false,
                ),
                resolved_mcp("external-1", "http", None, false, true),
            ],
            skills: Vec::new(),
            local_connector_requirements: Vec::new(),
        })
        .expect("policy")
    }

    fn task() -> TaskRecord {
        let now = now_rfc3339();
        TaskRecord {
            id: "task-1".to_string(),
            title: "Task".to_string(),
            description: None,
            objective: "Objective".to_string(),
            input_payload: None,
            status: TaskStatus::Ready,
            priority: 0,
            tags: Vec::new(),
            default_model_config_id: None,
            memory_thread_id: "thread-1".to_string(),
            tenant_id: "tenant-1".to_string(),
            subject_id: "owner-1".to_string(),
            project_id: "public".to_string(),
            task_profile: "default".to_string(),
            creator_user_id: Some("owner-1".to_string()),
            creator_username: None,
            creator_display_name: None,
            owner_user_id: Some("owner-1".to_string()),
            owner_username: None,
            owner_display_name: None,
            result_summary: None,
            process_log: None,
            last_run_id: None,
            schedule: TaskScheduleConfig::default(),
            parent_task_id: None,
            source_run_id: None,
            source_session_id: None,
            source_turn_id: None,
            source_user_message_id: None,
            prerequisite_task_ids: Vec::new(),
            task_tool_state: TaskToolState::default(),
            mcp_config: TaskMcpConfig {
                enabled: false,
                enabled_builtin_kinds: vec![
                    "CodeMaintainerRead".to_string(),
                    "CodeMaintainerWrite".to_string(),
                ],
                external_mcp_config_ids: vec!["external-1".to_string(), "revoked".to_string()],
                ..TaskMcpConfig::default()
            },
            created_at: now.clone(),
            updated_at: now,
            deleted_at: None,
        }
    }

    #[test]
    fn ai_selectable_sets_exclude_required_and_unavailable_capabilities() {
        let policy = policy();
        assert_eq!(
            policy.selectable_builtin_kind_names(),
            vec!["CodeMaintainerRead".to_string()]
        );
        assert_eq!(
            policy.selectable_external_mcp_ids(),
            vec!["external-1".to_string()]
        );
    }

    #[test]
    fn runtime_injects_required_and_intersects_saved_optional_selection() {
        let mut task = task();
        policy().apply_to_task(&mut task).expect("apply policy");
        assert!(task.mcp_config.enabled);
        assert_eq!(
            task.mcp_config.enabled_builtin_kinds,
            vec![
                "CodeMaintainerRead".to_string(),
                "TaskManager".to_string(),
                "AskUser".to_string(),
            ]
        );
        assert_eq!(
            task.mcp_config.external_mcp_config_ids,
            vec!["external-1".to_string()]
        );
    }

    #[test]
    fn write_validation_rejects_required_and_unavailable_selection() {
        let mut config = TaskMcpConfig {
            enabled_builtin_kinds: vec!["TaskManager".to_string()],
            ..TaskMcpConfig::default()
        };
        assert!(policy().validate_optional_config(&config).is_err());
        config.enabled_builtin_kinds = vec!["CodeMaintainerWrite".to_string()];
        assert!(policy().validate_optional_config(&config).is_err());
    }

    #[test]
    fn local_connector_user_mcp_requires_complete_execution_reference() {
        let mut incomplete = resolved_mcp(
            "local-user-incomplete",
            "local_connector_stdio",
            None,
            false,
            true,
        );
        incomplete.resource.source_kind = LOCAL_CONNECTOR_DISCOVERED_SOURCE_KIND.to_string();
        incomplete.resource.owner_kind = "user".to_string();
        incomplete.resource.runtime.local_connector = Some(LocalConnectorRef {
            device_id: Some("device-1".to_string()),
            workspace_id: None,
            manifest_id: None,
            requires_online: true,
            ..LocalConnectorRef::default()
        });
        let mut complete = incomplete.clone();
        complete.resource.id = "local-user-complete".to_string();
        complete.binding.resource_id = complete.resource.id.clone();
        complete.resource.runtime.local_connector = Some(LocalConnectorRef {
            manifest_id: Some("manifest-1".to_string()),
            ..incomplete
                .resource
                .runtime
                .local_connector
                .clone()
                .expect("local connector reference")
        });
        let policy = TaskRunnerCapabilityPolicy::new(ResolvedAgentCapabilities {
            agent_key: SystemAgentKey::TaskRunnerRunPhase.as_str().to_string(),
            owner_user_id: "owner-1".to_string(),
            policy_revision: "revision-local".to_string(),
            generated_at: "now".to_string(),
            agent_enabled: true,
            mcps: vec![incomplete, complete],
            skills: Vec::new(),
            local_connector_requirements: Vec::new(),
        })
        .expect("policy");

        assert_eq!(
            policy.selectable_external_mcp_ids(),
            vec!["local-user-complete".to_string()]
        );
    }
}
