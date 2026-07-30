// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_plugin_management_sdk::PluginComponentKind;
use serde::Serialize;
use serde_json::Value;

use super::{is_task_runner_execution_agent, TaskRunnerCapabilityPolicy};

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

impl TaskRunnerCapabilityPolicy {
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
}
