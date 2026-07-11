// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

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
    pub(in crate::mcp_server) schedule: Option<TaskScheduleConfig>,
    #[serde(default)]
    pub(in crate::mcp_server) enabled_builtin_kinds: Option<Vec<String>>,
    #[serde(default)]
    pub(in crate::mcp_server) external_mcp_config_ids: Option<Vec<String>>,
    #[serde(default)]
    pub(in crate::mcp_server) prerequisite_task_ids: Option<Vec<String>>,
    #[serde(default)]
    pub(in crate::mcp_server) mcp_config: Option<TaskMcpConfig>,
}

impl CreateTaskArgs {
    pub(in crate::mcp_server) fn into_request(self) -> Result<CreateTaskRequest, String> {
        let mut mcp_config = self.mcp_config;
        if let Some(enabled_builtin_kinds) = self.enabled_builtin_kinds {
            let normalized = normalize_mcp_builtin_kind_names(enabled_builtin_kinds)?;
            let config = mcp_config.get_or_insert_with(task_mcp_config_for_explicit_tool_selection);
            config.enabled = true;
            config.enabled_builtin_kinds = normalized;
        }
        if let Some(external_mcp_config_ids) = self.external_mcp_config_ids {
            let config = mcp_config.get_or_insert_with(task_mcp_config_for_explicit_tool_selection);
            config.enabled = true;
            config.external_mcp_config_ids =
                normalize_external_mcp_config_ids(external_mcp_config_ids);
        }
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
            mcp_config,
            prerequisite_task_ids: self.prerequisite_task_ids,
        })
    }
}

pub(in crate::mcp_server) fn task_mcp_config_for_explicit_tool_selection() -> TaskMcpConfig {
    TaskMcpConfig {
        enabled_builtin_kinds: Vec::new(),
        ..TaskMcpConfig::default()
    }
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
    pub(in crate::mcp_server) execution_group_id: Option<String>,
    #[serde(default)]
    pub(in crate::mcp_server) tasks: Vec<CreateProjectExecutionTaskItem>,
}

#[derive(Debug, Deserialize)]
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
    pub(in crate::mcp_server) enabled_builtin_kinds: Option<Vec<String>>,
    #[serde(default)]
    pub(in crate::mcp_server) external_mcp_config_ids: Option<Vec<String>>,
    #[serde(default)]
    pub(in crate::mcp_server) prerequisite_refs: Vec<String>,
    #[serde(default)]
    pub(in crate::mcp_server) prerequisite_task_ids: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub(in crate::mcp_server) struct CreateTaskWithPrerequisitesItem {
    pub(in crate::mcp_server) client_ref: String,
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
    pub(in crate::mcp_server) schedule: Option<TaskScheduleConfig>,
    #[serde(default)]
    pub(in crate::mcp_server) enabled_builtin_kinds: Option<Vec<String>>,
    #[serde(default)]
    pub(in crate::mcp_server) external_mcp_config_ids: Option<Vec<String>>,
    #[serde(default)]
    pub(in crate::mcp_server) prerequisite_refs: Vec<String>,
    #[serde(default)]
    pub(in crate::mcp_server) prerequisite_task_ids: Vec<String>,
}

pub(in crate::mcp_server) fn normalize_external_mcp_config_ids(values: Vec<String>) -> Vec<String> {
    normalize_unique_ids(values)
}

fn normalize_unique_ids(values: Vec<String>) -> Vec<String> {
    let mut out = Vec::new();
    for value in values {
        let trimmed = value.trim();
        if trimmed.is_empty() || out.iter().any(|item| item == trimmed) {
            continue;
        }
        out.push(trimmed.to_string());
    }
    out
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
