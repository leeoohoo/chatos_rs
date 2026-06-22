use serde_json::{json, Value};

use crate::auth::CurrentUser;
use crate::models::{ModelConfigRecord, TaskRecord};

use super::super::chatos_async_planner::planner_agent_tool_allowed;
use super::super::McpToolProfile;

pub(crate) fn agent_tool_allowed(name: &str) -> bool {
    matches!(
        name,
        "list_tasks"
            | "get_task"
            | "get_task_stats"
            | "create_task"
            | "list_mcp_builtin_catalog"
            | "create_tasks_with_prerequisites"
            | "update_task"
            | "set_task_prerequisites"
            | "cancel_task"
            | "wait_for_task_completion"
            | "get_task_dependency_graph"
            | "delete_task"
            | "batch_update_task_status"
            | "batch_delete_tasks"
            | "list_model_configs"
            | "get_model_config"
            | "list_runs"
            | "get_run"
            | "start_task_run"
            | "batch_start_task_runs"
            | "get_task_memory_context"
            | "list_task_memory_records"
            | "summarize_task_memory"
            | "cancel_run"
            | "retry_run"
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
    }
}

pub(crate) fn task_creator_filter(current_user: &CurrentUser) -> Option<String> {
    (!current_user.is_admin()).then(|| current_user.id.clone())
}

pub(crate) fn ensure_task_owner(
    task: &TaskRecord,
    current_user: &CurrentUser,
) -> Result<(), String> {
    if current_user.is_admin() {
        return Ok(());
    }
    if task.creator_user_id.as_deref() == Some(current_user.id.as_str()) {
        return Ok(());
    }
    Err("当前 agent 无权访问该任务".to_string())
}

pub(crate) fn require_admin_tool(current_user: &CurrentUser) -> Result<(), String> {
    if current_user.is_admin() {
        Ok(())
    } else {
        Err("当前 agent 无权调用管理员工具".to_string())
    }
}

pub(crate) fn tasks_for_external_mcp(tasks: Vec<TaskRecord>) -> Value {
    Value::Array(tasks.into_iter().map(task_for_external_mcp).collect())
}

pub(crate) fn task_for_external_mcp(task: TaskRecord) -> Value {
    let mut value = json!(task);
    remove_process_log_field(&mut value);
    value
}

pub(crate) fn remove_process_log_field(value: &mut Value) {
    if let Some(object) = value.as_object_mut() {
        object.remove("process_log");
    }
}

pub(crate) fn model_configs_for_user(
    models: Vec<ModelConfigRecord>,
    current_user: &CurrentUser,
) -> Vec<Value> {
    models
        .into_iter()
        .filter(|model| model_visible_to_user(model, current_user))
        .map(|model| model_config_for_user(model, current_user))
        .collect()
}

pub(crate) fn model_config_for_user(model: ModelConfigRecord, current_user: &CurrentUser) -> Value {
    if current_user.is_admin() {
        return json!(model);
    }
    let mut value = json!(model);
    if let Some(object) = value.as_object_mut() {
        object.insert("api_key".to_string(), Value::String(String::new()));
    }
    value
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

pub(crate) fn model_visible_to_user(
    model: &ModelConfigRecord,
    current_user: &CurrentUser,
) -> bool {
    current_user.can_access_owned_resource(model.owner_user_id.as_deref())
}
