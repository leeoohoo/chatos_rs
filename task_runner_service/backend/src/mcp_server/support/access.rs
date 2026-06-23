use serde_json::{json, Value};

use crate::auth::CurrentUser;
use crate::models::{ExternalMcpConfigRecord, ModelConfigRecord, TaskRecord};

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
            | "list_external_mcp_configs"
            | "create_tasks_with_prerequisites"
            | "update_task"
            | "set_task_prerequisites"
            | "cancel_task"
            | "wait_for_task_completion"
            | "get_task_dependency_graph"
            | "delete_task"
            | "batch_update_task_status"
            | "batch_delete_tasks"
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

pub(crate) fn external_mcp_configs_for_user(
    configs: Vec<ExternalMcpConfigRecord>,
    current_user: &CurrentUser,
) -> Vec<Value> {
    configs
        .into_iter()
        .filter(|config| config.enabled)
        .filter(|config| external_mcp_config_visible_to_user(config, current_user))
        .map(external_mcp_config_for_external_mcp)
        .collect()
}

fn external_mcp_config_visible_to_user(
    config: &ExternalMcpConfigRecord,
    current_user: &CurrentUser,
) -> bool {
    let owner = config
        .owner_user_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .or_else(|| config.creator_user_id.as_deref());
    current_user.can_access_owned_resource(owner)
}

fn external_mcp_config_for_external_mcp(config: ExternalMcpConfigRecord) -> Value {
    let endpoint = if config.transport == "http" {
        config.url.clone().unwrap_or_default()
    } else {
        std::iter::once(config.command.clone().unwrap_or_default())
            .chain(config.args.clone())
            .map(|item| item.trim().to_string())
            .filter(|item| !item.is_empty())
            .collect::<Vec<_>>()
            .join(" ")
    };
    json!({
        "id": config.id,
        "name": config.name,
        "transport": config.transport,
        "enabled": config.enabled,
        "endpoint": endpoint,
    })
}

pub(crate) fn agent_tool_allowed_for_profile(name: &str, tool_profile: McpToolProfile) -> bool {
    match tool_profile {
        McpToolProfile::Default => agent_tool_allowed(name),
        McpToolProfile::ChatosAsyncPlanner => planner_agent_tool_allowed(name),
    }
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
        .or_else(|| task.creator_user_id.as_deref());
    if owner_user_id == Some(effective_owner_user_id(current_user)?) {
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
    enabled_model_configs_for_user(models, current_user)
        .into_iter()
        .map(|model| model_config_for_user(model, current_user))
        .collect()
}

pub(crate) fn model_config_for_user(
    model: ModelConfigRecord,
    _current_user: &CurrentUser,
) -> Value {
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

pub(crate) fn enabled_model_configs_for_user(
    models: Vec<ModelConfigRecord>,
    current_user: &CurrentUser,
) -> Vec<ModelConfigRecord> {
    models
        .into_iter()
        .filter(|model| model_visible_to_user(model, current_user))
        .filter(|model| model.enabled)
        .collect()
}

pub(crate) fn model_visible_to_user(model: &ModelConfigRecord, current_user: &CurrentUser) -> bool {
    let Some(expected_owner_user_id) = current_user.effective_owner_user_id() else {
        return false;
    };
    model
        .owner_user_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        == Some(expected_owner_user_id)
}

pub(crate) fn select_model_config_id_for_task(
    models: &[ModelConfigRecord],
    title: &str,
    objective: &str,
    description: Option<&str>,
    tags: &[String],
) -> Option<String> {
    let haystack = task_model_selection_text(title, objective, description, tags);
    models
        .iter()
        .max_by_key(|model| model_task_match_score(model, haystack.as_str()))
        .map(|model| model.id.clone())
}

fn task_model_selection_text(
    title: &str,
    objective: &str,
    description: Option<&str>,
    tags: &[String],
) -> String {
    let mut parts = vec![title, objective];
    if let Some(description) = description {
        parts.push(description);
    }
    let mut text = parts.join(" ").to_ascii_lowercase();
    for tag in tags {
        text.push(' ');
        text.push_str(tag.as_str());
    }
    text.to_ascii_lowercase()
}

fn model_task_match_score(model: &ModelConfigRecord, haystack: &str) -> usize {
    let usage_score = text_match_score(model.usage_scenario.as_deref(), haystack, 5);
    let name_score = text_match_score(Some(model.name.as_str()), haystack, 2);
    let model_score = text_match_score(Some(model.model.as_str()), haystack, 1);
    let usage_bonus = model
        .usage_scenario
        .as_deref()
        .map(str::trim)
        .is_some_and(|value| !value.is_empty()) as usize;
    usage_score + name_score + model_score + usage_bonus
}

fn text_match_score(value: Option<&str>, haystack: &str, weight: usize) -> usize {
    let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) else {
        return 0;
    };
    value
        .split(|ch: char| !ch.is_alphanumeric())
        .map(str::trim)
        .filter(|token| token.chars().count() >= 2)
        .filter(|token| haystack.contains(token.to_ascii_lowercase().as_str()))
        .count()
        * weight
}
