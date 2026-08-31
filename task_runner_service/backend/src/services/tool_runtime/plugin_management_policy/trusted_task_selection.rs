// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::{HashMap, HashSet};

use chatos_plugin_management_sdk::{
    PluginAvailabilityStatus, PluginInstallStatus, PluginRequirementStatus, ResolvedPlugin,
    SelectedPluginRef, TaskPluginConfig,
};

use crate::auth::CurrentUser;
use crate::models::{
    normalize_project_id, normalize_task_profile, now_rfc3339, task_runner_agent_key_for,
    CreateTaskPluginHint, CreateTaskRequest, TaskPluginSelectionAudit, TaskScheduleConfig,
    TaskSelectedPluginSnapshot,
};

use super::{TaskRunnerCapabilityPolicy, TaskScheduleModeExt, TaskService};

#[derive(Debug, Clone, Default)]
pub(crate) struct TrustedTaskPluginSelection {
    pub(crate) plugin_config: TaskPluginConfig,
    pub(crate) audit: Option<TaskPluginSelectionAudit>,
}

impl TaskService {
    pub(crate) async fn resolve_trusted_task_plugin_selection(
        &self,
        input: &CreateTaskRequest,
        hints: &[CreateTaskPluginHint],
        current_user: &CurrentUser,
    ) -> Result<TrustedTaskPluginSelection, String> {
        let owner_user_id = current_user.effective_owner_user_id().ok_or_else(|| {
            "current Agent is missing owner scope for Plugin selection".to_string()
        })?;
        let project_id = normalize_project_id(input.project_id.clone());
        let task_profile = normalize_task_profile(input.task_profile.as_deref())?;
        let requires_execution = input
            .mcp_config
            .as_ref()
            .and_then(|config| config.requires_execution)
            .unwrap_or(true);
        let agent_key = task_runner_agent_key_for(task_profile.as_str(), requires_execution);
        let schedule = input
            .schedule
            .clone()
            .unwrap_or_else(TaskScheduleConfig::default);
        let Some(policy) = self
            .resolve_task_runner_policy_for_agent_project(
                Some(current_user),
                Some(owner_user_id),
                agent_key,
                project_id.as_deref(),
                Some(task_profile.as_str()),
                Some(schedule.mode.mode_key()),
            )
            .await?
        else {
            if hints.is_empty() {
                return Ok(TrustedTaskPluginSelection::default());
            }
            return Err(
                "Plugin Management policy is required to resolve task plugin_hints".to_string(),
            );
        };
        policy.plugin_selection_from_hints(hints)
    }
}

impl TaskRunnerCapabilityPolicy {
    pub(crate) fn plugin_selection_from_hints(
        &self,
        hints: &[CreateTaskPluginHint],
    ) -> Result<TrustedTaskPluginSelection, String> {
        let mut selected_plugins = Vec::new();
        let mut selected_ids = HashSet::new();
        let mut reasons_by_plugin_id = HashMap::new();
        for hint in hints {
            let plugin_key = hint.plugin_key.trim();
            let plugin = self
                .selectable_plugins()
                .into_iter()
                .find(|plugin| plugin.catalog.plugin_key.eq_ignore_ascii_case(plugin_key))
                .ok_or_else(|| {
                    format!(
                        "Plugin is not selectable for {}: {plugin_key}",
                        self.agent_key()
                    )
                })?;
            if selected_ids.insert(plugin.catalog.id.clone()) {
                selected_plugins.push(selected_plugin_ref(plugin.catalog.id.as_str()));
                reasons_by_plugin_id.insert(
                    plugin.catalog.id.clone(),
                    hint.reason
                        .as_deref()
                        .map(str::trim)
                        .filter(|reason| !reason.is_empty())
                        .map(ToOwned::to_owned),
                );
            }
        }
        for plugin in self.capabilities.required_plugins().filter(|plugin| {
            plugin.available
                || plugin.status
                    == chatos_plugin_management_sdk::PluginAvailabilityStatus::PartiallyAvailable
        }) {
            if selected_ids.insert(plugin.catalog.id.clone()) {
                selected_plugins.push(selected_plugin_ref(plugin.catalog.id.as_str()));
                reasons_by_plugin_id.insert(plugin.catalog.id.clone(), None);
            }
        }
        selected_plugins.sort_by(|left, right| left.plugin_id.cmp(&right.plugin_id));
        let config = TaskPluginConfig {
            selected_plugins,
            command_invocations: Vec::new(),
        };
        self.validate_plugin_config(&config)?;
        if config.selected_plugins.is_empty() {
            return Ok(TrustedTaskPluginSelection {
                plugin_config: config,
                audit: None,
            });
        }

        let expected_device_id = self.runtime_context().device_id.as_deref();
        let selection_context_revision = self
            .runtime_context()
            .project_context_revision
            .clone()
            .unwrap_or_else(|| format!("user-device-policy:{}", self.policy_revision()));
        let plugins = config
            .selected_plugins
            .iter()
            .map(|selected| {
                let plugin = self
                    .capabilities
                    .plugins
                    .iter()
                    .find(|plugin| plugin.catalog.id == selected.plugin_id)
                    .ok_or_else(|| {
                        format!(
                            "task_plugin_unavailable: selected Plugin snapshot is missing: {}",
                            selected.plugin_id
                        )
                    })?;
                selected_plugin_snapshot(
                    plugin,
                    expected_device_id,
                    reasons_by_plugin_id
                        .get(selected.plugin_id.as_str())
                        .cloned()
                        .flatten(),
                )
            })
            .collect::<Result<Vec<_>, String>>()?;

        Ok(TrustedTaskPluginSelection {
            plugin_config: config,
            audit: Some(TaskPluginSelectionAudit {
                selection_source: if hints.is_empty() {
                    "required_policy".to_string()
                } else {
                    "task_plugin_hints".to_string()
                },
                policy_revision: self.policy_revision().to_string(),
                selected_at: now_rfc3339(),
                project_context_revision: selection_context_revision,
                plugins,
            }),
        })
    }

    pub(crate) fn validate_task_plugin_selection_for_run(
        &self,
        task: &crate::models::TaskRecord,
    ) -> Result<(), String> {
        let mut selected_ids = task
            .plugin_config
            .selected_plugins
            .iter()
            .map(|selected| selected.plugin_id.as_str())
            .collect::<HashSet<_>>();
        selected_ids.extend(
            self.capabilities
                .required_plugins()
                .map(|plugin| plugin.catalog.id.as_str()),
        );
        if selected_ids.is_empty() {
            return Ok(());
        }
        let audit = task.plugin_selection_audit.as_ref().ok_or_else(|| {
            "task_plugin_unavailable: selected Plugin audit snapshot is missing".to_string()
        })?;
        let expected_device_id = self.runtime_context().device_id.as_deref();
        let audit_ids = audit
            .plugins
            .iter()
            .map(|snapshot| snapshot.plugin_id.as_str())
            .collect::<HashSet<_>>();
        if selected_ids != audit_ids {
            return Err(
                "task_plugin_unavailable: Task Plugin config and audit snapshot do not match"
                    .to_string(),
            );
        }
        for snapshot in &audit.plugins {
            if expected_device_id.is_some_and(|device_id| snapshot.device_id != device_id) {
                return Err(format!(
                    "task_plugin_unavailable: Local Connector device changed for Plugin {}",
                    snapshot.plugin_key
                ));
            }
            let plugin = self
                .capabilities
                .plugins
                .iter()
                .find(|plugin| plugin.catalog.id == snapshot.plugin_id)
                .ok_or_else(|| {
                    format!(
                        "task_plugin_unavailable: Plugin is no longer present in policy: {}",
                        snapshot.plugin_key
                    )
                })?;
            let current =
                selected_plugin_snapshot(plugin, expected_device_id, snapshot.reason.clone())?;
            if current.plugin_key != snapshot.plugin_key
                || current.release_id != snapshot.release_id
                || current.version != snapshot.version
                || current.artifact_sha256 != snapshot.artifact_sha256
                || current.device_id != snapshot.device_id
            {
                return Err(format!(
                    "task_plugin_unavailable: Plugin release changed after Task creation: {}",
                    snapshot.plugin_key
                ));
            }
        }
        self.validate_plugin_config(&task.plugin_config)
            .map_err(|error| format!("task_plugin_unavailable: {error}"))
    }

    pub(crate) fn refresh_task_plugin_selection_for_manual_retry(
        &self,
        task: &mut crate::models::TaskRecord,
    ) -> Result<bool, String> {
        let expected_device_id = self.runtime_context().device_id.as_deref();
        let selection_context_revision = self
            .runtime_context()
            .project_context_revision
            .clone()
            .unwrap_or_else(|| format!("user-device-policy:{}", self.policy_revision()));
        let reasons_by_plugin_id = task
            .plugin_selection_audit
            .as_ref()
            .map(|audit| {
                audit
                    .plugins
                    .iter()
                    .map(|snapshot| (snapshot.plugin_id.clone(), snapshot.reason.clone()))
                    .collect::<HashMap<_, _>>()
            })
            .unwrap_or_default();
        let mut selected_ids = task
            .plugin_config
            .selected_plugins
            .iter()
            .map(|selected| selected.plugin_id.clone())
            .collect::<HashSet<_>>();
        selected_ids.extend(
            self.capabilities
                .required_plugins()
                .map(|plugin| plugin.catalog.id.clone()),
        );
        if selected_ids.is_empty() {
            let changed = task.plugin_selection_audit.take().is_some();
            return Ok(changed);
        }

        self.validate_plugin_config(&task.plugin_config)
            .map_err(|error| format!("task_plugin_unavailable: {error}"))?;
        let mut selected_ids = selected_ids.into_iter().collect::<Vec<_>>();
        selected_ids.sort();
        let plugins = selected_ids
            .iter()
            .map(|plugin_id| {
                let plugin = self
                    .capabilities
                    .plugins
                    .iter()
                    .find(|plugin| plugin.catalog.id == *plugin_id)
                    .ok_or_else(|| {
                        format!(
                            "task_plugin_unavailable: Plugin is no longer present in policy: {plugin_id}"
                        )
                    })?;
                selected_plugin_snapshot(
                    plugin,
                    expected_device_id,
                    reasons_by_plugin_id
                        .get(plugin_id.as_str())
                        .cloned()
                        .flatten(),
                )
            })
            .collect::<Result<Vec<_>, String>>()?;
        let refreshed = TaskPluginSelectionAudit {
            selection_source: "manual_retry_refresh".to_string(),
            policy_revision: self.policy_revision().to_string(),
            selected_at: now_rfc3339(),
            project_context_revision: selection_context_revision,
            plugins,
        };
        let changed = task.plugin_selection_audit.as_ref().is_none_or(|current| {
            current.policy_revision != refreshed.policy_revision
                || current.project_context_revision != refreshed.project_context_revision
                || current.plugins != refreshed.plugins
        });
        if changed {
            task.plugin_selection_audit = Some(refreshed);
        }
        Ok(changed)
    }
}

fn selected_plugin_snapshot(
    plugin: &ResolvedPlugin,
    expected_device_id: Option<&str>,
    reason: Option<String>,
) -> Result<TaskSelectedPluginSnapshot, String> {
    let release = plugin.release.as_ref().ok_or_else(|| {
        format!(
            "task_plugin_unavailable: Plugin Release snapshot is missing: {}",
            plugin.catalog.plugin_key
        )
    })?;
    let installation = plugin.installation.as_ref().ok_or_else(|| {
        format!(
            "task_plugin_unavailable: Local Connector installation is missing: {}",
            plugin.catalog.plugin_key
        )
    })?;
    if release.revoked_at.is_some()
        || !installation.active
        || installation.install_status != PluginInstallStatus::Installed
        || !matches!(
            installation.availability_status,
            PluginAvailabilityStatus::Ready | PluginAvailabilityStatus::PartiallyAvailable
        )
        || installation.dependency_status != PluginRequirementStatus::Satisfied
        || installation.permission_status != PluginRequirementStatus::Satisfied
        || installation.auth_status != PluginRequirementStatus::Satisfied
    {
        return Err(format!(
            "task_plugin_unavailable: Plugin installation, dependency, permission, or auth state is not ready: {}",
            plugin.catalog.plugin_key
        ));
    }
    if expected_device_id.is_some_and(|device_id| installation.device_id != device_id)
        || installation.plugin_id != plugin.catalog.id
        || installation.release_id != release.id
        || installation.version != release.version
        || installation.artifact_sha256 != release.artifact_sha256
    {
        return Err(format!(
            "task_plugin_unavailable: installed Plugin identity does not match the immutable Release: {}",
            plugin.catalog.plugin_key
        ));
    }
    Ok(TaskSelectedPluginSnapshot {
        plugin_id: plugin.catalog.id.clone(),
        plugin_key: plugin.catalog.plugin_key.clone(),
        display_name: plugin.catalog.display_name.clone(),
        release_id: release.id.clone(),
        version: release.version.clone(),
        artifact_sha256: release.artifact_sha256.clone(),
        device_id: installation.device_id.clone(),
        reason,
    })
}

fn selected_plugin_ref(plugin_id: &str) -> SelectedPluginRef {
    SelectedPluginRef {
        plugin_id: plugin_id.to_string(),
        selected_skill_ids: Vec::new(),
        selected_command_ids: Vec::new(),
        selected_agent_ids: Vec::new(),
    }
}
