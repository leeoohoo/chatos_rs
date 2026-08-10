// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeMap;

use chatos_plugin_management_sdk::{PluginComponentKind, PluginExecutionHost};
use serde::Serialize;
use serde_json::Value;

use super::{is_task_runner_execution_agent, TaskRunnerCapabilityPolicy};

#[derive(Debug, Clone, Serialize)]
pub(crate) struct SelectablePluginView {
    pub id: String,
    pub plugin_key: String,
    pub display_name: String,
    pub description: String,
    pub version: String,
    pub release_id: String,
    pub artifact_sha256: String,
    pub device_id: Option<String>,
    pub execution_type: String,
    pub requires_device: bool,
    pub component_hosts: BTreeMap<String, PluginExecutionHost>,
    pub component_keys: Vec<String>,
    pub components: Vec<SelectablePluginComponentView>,
    pub commands: Vec<SelectablePluginCommandView>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct SelectablePluginComponentView {
    pub component_key: String,
    pub kind: PluginComponentKind,
    pub execution_host: PluginExecutionHost,
    pub available: bool,
    pub status: chatos_plugin_management_sdk::PluginAvailabilityStatus,
    pub reason: Option<String>,
    pub content_sha256: Option<String>,
    pub prepare_provider: String,
    pub requires_workspace: bool,
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

impl TaskRunnerCapabilityPolicy {
    pub(crate) fn selectable_plugin_views(&self) -> Vec<SelectablePluginView> {
        self.selectable_plugins()
            .into_iter()
            .filter_map(|plugin| {
                let release = plugin.release.as_ref()?;
                let component_hosts = plugin
                    .components
                    .iter()
                    .filter(|component| component.component.kind != PluginComponentKind::Agent)
                    .map(|component| {
                        (
                            component.component.component_key.clone(),
                            component.component.execution_host,
                        )
                    })
                    .collect::<BTreeMap<_, _>>();
                let execution_type = plugin_execution_type(component_hosts.values().copied());
                let local_portable_execution = self.portable_uses_local;
                let components = plugin
                    .components
                    .iter()
                    .filter(|component| component.component.kind != PluginComponentKind::Agent)
                    .map(|component| {
                        let snapshot = plugin.component_snapshots.iter().find(|snapshot| {
                            snapshot.component.component_key == component.component.component_key
                        });
                        let uses_local = component.component.execution_host
                            == PluginExecutionHost::Local
                            || (component.component.execution_host
                                == PluginExecutionHost::Portable
                                && local_portable_execution);
                        SelectablePluginComponentView {
                            component_key: component.component.component_key.clone(),
                            kind: component.component.kind,
                            execution_host: component.component.execution_host,
                            available: component.available,
                            status: component.status,
                            reason: component.reason.clone(),
                            content_sha256: snapshot
                                .map(|snapshot| snapshot.content_sha256.clone()),
                            prepare_provider: if uses_local {
                                "local_connector".to_string()
                            } else {
                                "task_runner_cloud".to_string()
                            },
                            requires_workspace: component
                                .component
                                .permissions
                                .iter()
                                .any(|permission| permission.permission.starts_with("workspace.")),
                        }
                    })
                    .collect::<Vec<_>>();
                Some(SelectablePluginView {
                    id: plugin.catalog.id.clone(),
                    plugin_key: plugin.catalog.plugin_key.clone(),
                    display_name: plugin.catalog.display_name.clone(),
                    description: plugin.catalog.description.clone(),
                    version: release.version.clone(),
                    release_id: release.id.clone(),
                    artifact_sha256: release.artifact_sha256.clone(),
                    device_id: plugin
                        .installation
                        .as_ref()
                        .map(|installation| installation.device_id.clone()),
                    requires_device: component_hosts.values().any(|host| {
                        *host == PluginExecutionHost::Local
                            || (*host == PluginExecutionHost::Portable && local_portable_execution)
                    }),
                    execution_type,
                    component_hosts,
                    component_keys: plugin
                        .components
                        .iter()
                        .filter(|component| {
                            component.available
                                && component.component.kind != PluginComponentKind::Agent
                        })
                        .map(|component| component.component.component_key.clone())
                        .collect(),
                    components,
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
                })
            })
            .collect()
    }
}

fn plugin_execution_type(hosts: impl Iterator<Item = PluginExecutionHost>) -> String {
    let hosts = hosts.collect::<std::collections::HashSet<_>>();
    if hosts.len() != 1 {
        return "hybrid".to_string();
    }
    match hosts
        .into_iter()
        .next()
        .unwrap_or(PluginExecutionHost::Local)
    {
        PluginExecutionHost::Cloud => "cloud",
        PluginExecutionHost::Local => "local",
        PluginExecutionHost::Portable => "portable",
    }
    .to_string()
}
