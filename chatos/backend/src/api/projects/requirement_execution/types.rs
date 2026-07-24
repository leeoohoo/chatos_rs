// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::models::memory_mapping_types::MemoryProjectContactDto;
pub(in crate::api::projects) use chatos_project_execution::{
    RequirementPlanItem, WorkItemPlanItem,
};

#[derive(Debug, Clone)]
pub(in crate::api::projects) struct SelectedContactRuntime {
    pub(in crate::api::projects) contact: MemoryProjectContactDto,
    pub(in crate::api::projects) task_runner_base_url: String,
    pub(in crate::api::projects) task_runner_agent_token: String,
}

#[derive(Debug, Clone)]
pub(in crate::api::projects) struct ExecutionLink {
    pub(in crate::api::projects) link_id: Option<String>,
    pub(in crate::api::projects) work_item_id: String,
    pub(in crate::api::projects) task_runner_task_id: String,
    pub(in crate::api::projects) task_runner_run_id: Option<String>,
    pub(in crate::api::projects) task_runner_status: Option<String>,
    pub(in crate::api::projects) source_session_id: Option<String>,
    pub(in crate::api::projects) source_user_message_id: Option<String>,
}
