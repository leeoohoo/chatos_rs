// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;
use chatos_plugin_management_sdk::{SelectedPluginRef, TaskPluginConfig};

#[derive(Debug, Default, Deserialize)]
pub(in crate::mcp_server) struct ListTasksArgs {
    #[serde(default)]
    pub(in crate::mcp_server) status: Option<TaskStatus>,
    #[serde(default)]
    pub(in crate::mcp_server) keyword: Option<String>,
    #[serde(default)]
    pub(in crate::mcp_server) tag: Option<String>,
    #[serde(default)]
    pub(in crate::mcp_server) model_config_id: Option<String>,
    #[serde(default)]
    pub(in crate::mcp_server) scheduled_only: Option<bool>,
    #[serde(default)]
    pub(in crate::mcp_server) parent_task_id: Option<String>,
    #[serde(default)]
    pub(in crate::mcp_server) source_run_id: Option<String>,
    #[serde(default)]
    pub(in crate::mcp_server) limit: Option<usize>,
    #[serde(default)]
    pub(in crate::mcp_server) offset: Option<usize>,
}

#[derive(Debug, Deserialize)]
pub(in crate::mcp_server) struct TaskIdArgs {
    pub(in crate::mcp_server) task_id: String,
}

#[derive(Debug, Deserialize)]
pub(in crate::mcp_server) struct CreateTaskArgs {
    pub(in crate::mcp_server) title: String,
    #[serde(default)]
    pub(in crate::mcp_server) description: Option<String>,
    pub(in crate::mcp_server) objective: String,
    #[serde(default)]
    pub(in crate::mcp_server) input_payload: Option<Value>,
    #[serde(default)]
    pub(in crate::mcp_server) priority: Option<i32>,
    #[serde(default)]
    pub(in crate::mcp_server) tags: Option<Vec<String>>,
    #[serde(default)]
    pub(in crate::mcp_server) default_model_config_id: Option<String>,
    #[serde(default)]
    pub(in crate::mcp_server) requires_execution: Option<bool>,
    #[serde(default)]
    pub(in crate::mcp_server) is_planning_task: Option<bool>,
    #[serde(default)]
    pub(in crate::mcp_server) schedule: Option<TaskScheduleConfig>,
    #[serde(default)]
    // Kept only to reject stale or handcrafted AI calls explicitly. MCP
    // capabilities are materialized from the Agent binding by the service.
    pub(in crate::mcp_server) enabled_builtin_kinds: Option<Vec<String>>,
    #[serde(default)]
    pub(in crate::mcp_server) external_mcp_config_ids: Option<Vec<String>>,
    #[serde(default)]
    pub(in crate::mcp_server) plugin_device_id: Option<String>,
    #[serde(default)]
    pub(in crate::mcp_server) plugin_workspace_id: Option<String>,
    #[serde(default)]
    pub(in crate::mcp_server) selected_plugins: Option<Vec<SelectedPluginRef>>,
    #[serde(default)]
    pub(in crate::mcp_server) prerequisite_task_ids: Option<Vec<String>>,
    #[serde(default)]
    pub(in crate::mcp_server) mcp_config: Option<TaskMcpConfig>,
}

impl CreateTaskArgs {
    pub(in crate::mcp_server) fn into_request(self) -> Result<CreateTaskRequest, String> {
        if self.enabled_builtin_kinds.is_some()
            || self.external_mcp_config_ids.is_some()
            || self.plugin_device_id.is_some()
            || self.plugin_workspace_id.is_some()
            || self.selected_plugins.is_some()
            || self.mcp_config.is_some()
        {
            return Err(
                "Tool capabilities and runtime routing are controlled by the program through Agent bindings and cannot be selected by AI"
                    .to_string(),
            );
        }
        let mcp_config = self
            .requires_execution
            .map(|requires_execution| TaskMcpRequestConfig {
                requires_execution: Some(requires_execution),
            });
        Ok(CreateTaskRequest {
            title: self.title,
            description: self.description,
            objective: self.objective,
            input_payload: self.input_payload,
            status: None,
            priority: self.priority,
            tags: self.tags,
            default_model_config_id: self.default_model_config_id,
            project_id: None,
            task_profile: None,
            tenant_id: None,
            subject_id: None,
            schedule: self.schedule,
            plugin_config: TaskPluginConfig::default(),
            mcp_config,
            prerequisite_task_ids: self.prerequisite_task_ids,
        })
    }
}

pub(in crate::mcp_server) fn reject_ai_runtime_config(
    mcp_config: Option<&TaskMcpRequestConfig>,
    plugin_config: Option<&TaskPluginConfig>,
) -> Result<(), String> {
    if mcp_config.is_some() || plugin_config.is_some() {
        return Err(
            "Tool capabilities and runtime routing are controlled by the program through Agent bindings and cannot be changed by AI"
                .to_string(),
        );
    }
    Ok(())
}

#[derive(Debug, Deserialize)]
pub(in crate::mcp_server) struct UpdateTaskArgs {
    pub(in crate::mcp_server) task_id: String,
    #[serde(default)]
    pub(in crate::mcp_server) patch: UpdateTaskRequest,
}

#[derive(Debug, Deserialize)]
pub(in crate::mcp_server) struct SetTaskPrerequisitesArgs {
    pub(in crate::mcp_server) task_id: String,
    #[serde(default)]
    pub(in crate::mcp_server) prerequisite_task_ids: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub(in crate::mcp_server) struct CancelTaskArgs {
    pub(in crate::mcp_server) task_id: String,
    pub(in crate::mcp_server) reason: String,
    #[serde(default)]
    pub(in crate::mcp_server) replacement_task_ids: Vec<String>,
}

impl CancelTaskArgs {
    pub(in crate::mcp_server) fn into_request(self) -> CancelTaskRequest {
        CancelTaskRequest {
            reason: self.reason,
            replacement_task_ids: self.replacement_task_ids,
        }
    }
}

#[derive(Debug, Default, Deserialize)]
pub(in crate::mcp_server) struct CreateTasksWithPrerequisitesArgs {
    #[serde(default)]
    pub(in crate::mcp_server) tasks: Vec<CreateTaskWithPrerequisitesItem>,
}

#[derive(Debug, Default, Deserialize)]
pub(in crate::mcp_server) struct CreateProjectExecutionTasksArgs {
    pub(in crate::mcp_server) project_id: String,
    pub(in crate::mcp_server) requirement_id: String,
    #[serde(default)]
    pub(in crate::mcp_server) tasks: Vec<CreateProjectExecutionTaskItem>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::mcp_server) struct CreateProjectExecutionTaskItem {
    pub(in crate::mcp_server) client_ref: String,
    pub(in crate::mcp_server) project_task_id: String,
    pub(in crate::mcp_server) title: String,
    #[serde(default)]
    pub(in crate::mcp_server) description: Option<String>,
    pub(in crate::mcp_server) objective: String,
    #[serde(default)]
    pub(in crate::mcp_server) input_payload: Option<Value>,
    #[serde(default)]
    pub(in crate::mcp_server) priority: Option<i32>,
    #[serde(default)]
    pub(in crate::mcp_server) tags: Option<Vec<String>>,
    #[serde(default)]
    pub(in crate::mcp_server) default_model_config_id: Option<String>,
    #[serde(default)]
    // Rejected if present; retained for explicit fail-closed compatibility.
    pub(in crate::mcp_server) enabled_builtin_kinds: Option<Vec<String>>,
    #[serde(default)]
    pub(in crate::mcp_server) external_mcp_config_ids: Option<Vec<String>>,
    #[serde(default)]
    pub(in crate::mcp_server) prerequisite_refs: Vec<String>,
    #[serde(default)]
    pub(in crate::mcp_server) context_refs: Vec<String>,
    #[serde(default)]
    pub(in crate::mcp_server) prerequisite_task_ids: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub(in crate::mcp_server) struct CreateTaskWithPrerequisitesItem {
    pub(in crate::mcp_server) client_ref: String,
    #[serde(flatten)]
    pub(in crate::mcp_server) task: CreateTaskArgs,
    #[serde(default)]
    pub(in crate::mcp_server) prerequisite_refs: Vec<String>,
    #[serde(default)]
    pub(in crate::mcp_server) context_refs: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub(in crate::mcp_server) struct BatchTaskStatusUpdateArgs {
    pub(in crate::mcp_server) task_ids: Vec<String>,
    pub(in crate::mcp_server) status: TaskStatus,
}

#[derive(Debug, Deserialize)]
pub(in crate::mcp_server) struct BatchTaskDeleteArgs {
    pub(in crate::mcp_server) task_ids: Vec<String>,
}

#[derive(Debug, Default, Deserialize)]
pub(in crate::mcp_server) struct BatchTaskRunArgs {
    pub(in crate::mcp_server) task_ids: Vec<String>,
    #[serde(default)]
    pub(in crate::mcp_server) model_config_id: Option<String>,
    #[serde(default)]
    pub(in crate::mcp_server) prompt_override: Option<String>,
}
