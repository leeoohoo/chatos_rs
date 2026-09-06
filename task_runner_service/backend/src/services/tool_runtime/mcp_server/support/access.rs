// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde_json::{json, Value};

use crate::auth::CurrentUser;
use crate::models::{ModelConfigRecord, TaskRecord, TaskStatus};

use super::super::chatos_async_planner::planner_agent_tool_allowed;
use super::super::{McpRequestContext, McpToolProfile};

pub(crate) fn agent_tool_allowed(name: &str) -> bool {
    matches!(
        name,
        "list_tasks"
            | "get_task"
            | "get_task_stats"
            | "create_task"
            | "create_tasks_with_prerequisites"
            | "update_task"
            | "set_task_prerequisites"
            | "cancel_task"
            | "wait_for_task_completion"
            | "get_task_dependency_graph"
            | "delete_task"
            | "batch_delete_tasks"
            | "list_runs"
            | "get_run"
            | "start_task_run"
            | "batch_start_task_runs"
            | "get_task_memory_context"
            | "list_task_memory_records"
            | "cancel_run"
            | "list_run_events"
            | "list_prompts"
            | "get_prompt"
            | "submit_prompt"
            | "cancel_prompt"
    )
}

pub(crate) fn agent_tool_allowed_for_profile(name: &str, tool_profile: McpToolProfile) -> bool {
    match tool_profile {
        McpToolProfile::Default => agent_tool_allowed(name),
        McpToolProfile::ChatosAsyncPlanner => planner_agent_tool_allowed(name),
        McpToolProfile::ProjectRequirementExecutionPlanner => matches!(
            name,
            "list_tasks"
                | "get_task"
                | "get_task_dependency_graph"
                | "create_project_execution_tasks"
        ),
    }
}

pub(crate) fn agent_tool_allowed_for_request_context(
    name: &str,
    request_context: &McpRequestContext,
) -> bool {
    agent_tool_allowed_for_profile(name, request_context.tool_profile())
}

pub(crate) fn reusable_chatos_async_task(task: &TaskRecord) -> bool {
    matches!(
        task.status,
        TaskStatus::Ready | TaskStatus::Queued | TaskStatus::Running
    )
}

pub(crate) fn ensure_task_startable_from_mcp(
    task: &TaskRecord,
    request_context: &McpRequestContext,
) -> Result<(), String> {
    if !matches!(task.status, TaskStatus::Draft | TaskStatus::Ready) {
        return Err(historical_task_read_only_message());
    }
    if task
        .last_run_id
        .as_deref()
        .map(str::trim)
        .is_some_and(|value| !value.is_empty())
    {
        return Err(historical_task_read_only_message());
    }
    if request_has_concrete_source(request_context)
        && !task_matches_request_source(task, request_context)
    {
        return Err(historical_task_read_only_message());
    }
    Ok(())
}

pub(crate) fn ensure_task_status_update_allowed_from_mcp(
    current_user: &CurrentUser,
) -> Result<(), String> {
    if current_user.is_admin() {
        return Ok(());
    }
    Err(
        "Chatos task tools cannot update task execution status directly. Create a new task for new work, or use cancel_task for obsolete tasks."
            .to_string(),
    )
}

fn request_has_concrete_source(request_context: &McpRequestContext) -> bool {
    non_empty(request_context.source_session_id.as_deref()).is_some()
        && (non_empty(request_context.source_user_message_id.as_deref()).is_some()
            || non_empty(request_context.source_turn_id.as_deref()).is_some())
}

fn task_matches_request_source(task: &TaskRecord, request_context: &McpRequestContext) -> bool {
    let Some(session_id) = non_empty(request_context.source_session_id.as_deref()) else {
        return false;
    };
    if non_empty(task.source_session_id.as_deref()) != Some(session_id) {
        return false;
    }
    if let Some(message_id) = non_empty(request_context.source_user_message_id.as_deref()) {
        return non_empty(task.source_user_message_id.as_deref()) == Some(message_id);
    }
    if let Some(turn_id) = non_empty(request_context.source_turn_id.as_deref()) {
        return non_empty(task.source_turn_id.as_deref()) == Some(turn_id);
    }
    false
}

fn non_empty(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}

fn historical_task_read_only_message() -> String {
    "Historical Task Runner tasks are read-only through Chatos task tools. Create a new task for current work, or use cancel_task to stop obsolete work.".to_string()
}

pub(crate) fn effective_owner_user_id(current_user: &CurrentUser) -> Result<&str, String> {
    current_user
        .effective_owner_user_id()
        .ok_or_else(|| "当前登录态缺少用户归属信息".to_string())
}

pub(crate) fn task_creator_filter(current_user: &CurrentUser) -> Result<Option<String>, String> {
    if current_user.is_admin() {
        return Ok(None);
    }
    Ok(Some(effective_owner_user_id(current_user)?.to_string()))
}

pub(crate) fn ensure_task_owner(
    task: &TaskRecord,
    current_user: &CurrentUser,
) -> Result<(), String> {
    if current_user.is_admin() {
        return Ok(());
    }
    let owner_user_id = task
        .owner_user_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .or(task.creator_user_id.as_deref());
    if owner_user_id == Some(effective_owner_user_id(current_user)?) {
        return Ok(());
    }
    Err("当前 agent 无权访问该任务".to_string())
}

pub(crate) fn tasks_for_agent_tool(tasks: Vec<TaskRecord>) -> Value {
    Value::Array(tasks.into_iter().map(task_for_agent_tool).collect())
}

pub(crate) fn task_for_agent_tool(task: TaskRecord) -> Value {
    value_for_agent_tool(json!(task))
}

pub(crate) fn value_for_agent_tool(mut value: Value) -> Value {
    remove_internal_task_fields(&mut value);
    value
}

pub(crate) fn remove_internal_task_fields(value: &mut Value) {
    match value {
        Value::Array(items) => {
            for item in items {
                remove_internal_task_fields(item);
            }
        }
        Value::Object(object) => {
            for field in [
                "process_log",
                "project_id",
                "tenant_id",
                "subject_id",
                "task_profile",
                "default_model_config_id",
                "model_config_id",
                "memory_thread_id",
                "creator_user_id",
                "creator_username",
                "creator_display_name",
                "owner_user_id",
                "owner_username",
                "owner_display_name",
                "source_session_id",
                "source_turn_id",
                "source_user_message_id",
                "agent_key",
                "plugin_config",
                "mcp_config",
                "plugin_snapshots",
                "effective_workspace_dir",
                "task_tool_state",
                "worker_id",
                "claim_token",
                "claim_until",
                "summary_job_run_id",
                "chatos_started_callback_delivery",
                "chatos_callback_delivery",
            ] {
                object.remove(field);
            }
            for item in object.values_mut() {
                remove_internal_task_fields(item);
            }
        }
        _ => {}
    }
}

pub(crate) fn model_has_cloud_runtime_credentials(model: &ModelConfigRecord) -> bool {
    !model.api_key.trim().is_empty() && !model.base_url.trim().is_empty()
}

pub(crate) fn filter_model_configs_for_user(
    models: Vec<ModelConfigRecord>,
    current_user: &CurrentUser,
) -> Vec<ModelConfigRecord> {
    models
        .into_iter()
        .filter(|model| model_visible_to_user(model, current_user))
        .collect()
}

pub(crate) fn model_visible_to_user(model: &ModelConfigRecord, current_user: &CurrentUser) -> bool {
    current_user.can_access_owned_resource(model.owner_user_id.as_deref())
}
