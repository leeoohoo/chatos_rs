// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::models::memory_mapping_types::MemoryProjectContactDto;

#[derive(Debug, Clone)]
pub(in crate::api::projects) struct RequirementPlanItem {
    pub(in crate::api::projects) id: String,
    pub(in crate::api::projects) title: String,
    pub(in crate::api::projects) status: String,
    pub(in crate::api::projects) parent_requirement_id: Option<String>,
}

#[derive(Debug, Clone)]
pub(in crate::api::projects) struct WorkItemPlanItem {
    pub(in crate::api::projects) id: String,
    pub(in crate::api::projects) requirement_id: String,
    pub(in crate::api::projects) title: String,
    pub(in crate::api::projects) description: Option<String>,
    pub(in crate::api::projects) task_runner_default_model_config_id: String,
    pub(in crate::api::projects) task_runner_enabled_tool_ids: Vec<String>,
    pub(in crate::api::projects) task_runner_skill_ids: Vec<String>,
    pub(in crate::api::projects) status: String,
    pub(in crate::api::projects) priority: i32,
    pub(in crate::api::projects) tags: Vec<String>,
    pub(in crate::api::projects) is_planning_task: bool,
}

#[derive(Debug, Clone)]
pub(in crate::api::projects) struct SelectedContactRuntime {
    pub(in crate::api::projects) contact: MemoryProjectContactDto,
    pub(in crate::api::projects) task_runner_base_url: String,
    pub(in crate::api::projects) task_runner_agent_token: String,
}

#[derive(Debug, Clone)]
pub(in crate::api::projects) struct CreatedExecutionTask {
    pub(in crate::api::projects) project_task_id: String,
    pub(in crate::api::projects) requirement_id: String,
    pub(in crate::api::projects) task_runner_task_id: String,
    pub(in crate::api::projects) task_runner_run_id: Option<String>,
    pub(in crate::api::projects) task_runner_status: String,
}

#[derive(Debug, Clone)]
pub(in crate::api::projects) struct ExecutionLink {
    pub(in crate::api::projects) work_item_id: String,
    pub(in crate::api::projects) task_runner_task_id: String,
    pub(in crate::api::projects) task_runner_run_id: Option<String>,
    pub(in crate::api::projects) task_runner_status: Option<String>,
    pub(in crate::api::projects) source_session_id: Option<String>,
    pub(in crate::api::projects) source_user_message_id: Option<String>,
}
