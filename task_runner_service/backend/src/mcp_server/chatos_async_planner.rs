use serde_json::Value;

use crate::models::{
    mcp_builtin_kind_guide, mcp_builtin_kind_values, now_rfc3339, CreateTaskRequest,
    ModelConfigRecord, TaskScheduleConfig, TaskScheduleMode, TaskStatus, UpdateTaskRequest,
};
use chatos_mcp_runtime::builtin_kind_by_any;

use super::support::{
    remove_tool_schema_property, set_schema_required_fields, set_tool_property_description,
};
use super::McpRequestContext;

pub(super) fn planner_agent_tool_allowed(name: &str) -> bool {
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
            | "wait_for_task_completion"
            | "get_task_dependency_graph"
    )
}

pub(super) fn planner_update_task_request(
    patch: UpdateTaskRequest,
) -> Result<UpdateTaskRequest, String> {
    if patch.status.is_some() {
        return Err("联系人异步模式不能通过 update_task 修改任务执行状态".to_string());
    }
    Ok(patch)
}

pub(super) fn planner_root_create_request(
    mut input: CreateTaskRequest,
) -> Result<CreateTaskRequest, String> {
    ensure_planner_required_fields(&input)?;
    input.status = Some(TaskStatus::Ready);
    input.schedule = Some(planner_schedule_contact_async_now(
        input.schedule.unwrap_or_default(),
    )?);
    Ok(input)
}

pub(super) fn require_chatos_async_source_context(
    request_context: &McpRequestContext,
) -> Result<(&str, &str), String> {
    let source_session_id = request_context
        .source_session_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            "Chatos async planner 缺少 source_session_id，拒绝创建无来源任务".to_string()
        })?;
    let source_user_message_id = request_context
        .source_user_message_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            "Chatos async planner 缺少 source_user_message_id，拒绝创建无来源任务".to_string()
        })?;
    Ok((source_session_id, source_user_message_id))
}

pub(super) fn planner_prerequisite_create_request(
    mut input: CreateTaskRequest,
) -> Result<CreateTaskRequest, String> {
    ensure_planner_required_fields(&input)?;
    input.status = Some(TaskStatus::Ready);
    input.schedule = Some(planner_schedule_contact_async_now(
        input.schedule.unwrap_or_default(),
    )?);
    Ok(input)
}

pub(super) fn ensure_planner_required_fields(input: &CreateTaskRequest) -> Result<(), String> {
    if input
        .default_model_config_id
        .as_deref()
        .map(str::trim)
        .is_none_or(|value| value.is_empty())
    {
        return Err("联系人异步任务必须指定 default_model_config_id".to_string());
    }
    let has_enabled_builtin_kinds = input.mcp_config.as_ref().is_some_and(|config| {
        config
            .enabled_builtin_kinds
            .iter()
            .any(|value| !value.trim().is_empty())
    });
    if !has_enabled_builtin_kinds {
        return Err("联系人异步任务必须至少选择一个 enabled_builtin_kinds".to_string());
    }
    Ok(())
}

pub(super) fn enrich_tool_schemas_for_async_planner(
    tools: &mut [Value],
    model_configs: &[ModelConfigRecord],
) {
    let enabled_models = model_configs
        .iter()
        .filter(|model| model.enabled)
        .cloned()
        .collect::<Vec<_>>();
    let task_model_description = planner_task_model_config_description(&enabled_models);
    let builtin_description = planner_builtin_mcp_kind_schema_description();
    for tool in tools {
        match tool.get("name").and_then(Value::as_str) {
            Some("create_task") => {
                set_schema_required_fields(
                    tool,
                    &["inputSchema", "required"],
                    &[
                        "title",
                        "objective",
                        "default_model_config_id",
                        "enabled_builtin_kinds",
                    ],
                );
                set_tool_property_description(
                    tool,
                    &["inputSchema", "properties", "default_model_config_id"],
                    task_model_description.clone(),
                );
                set_tool_property_description(
                    tool,
                    &["inputSchema", "properties", "enabled_builtin_kinds"],
                    builtin_description.clone(),
                );
            }
            Some("create_tasks_with_prerequisites") => {
                set_schema_required_fields(
                    tool,
                    &["inputSchema", "properties", "tasks", "items", "required"],
                    &[
                        "client_ref",
                        "title",
                        "objective",
                        "default_model_config_id",
                        "enabled_builtin_kinds",
                    ],
                );
                set_tool_property_description(
                    tool,
                    &[
                        "inputSchema",
                        "properties",
                        "tasks",
                        "items",
                        "properties",
                        "default_model_config_id",
                    ],
                    task_model_description.clone(),
                );
                set_tool_property_description(
                    tool,
                    &[
                        "inputSchema",
                        "properties",
                        "tasks",
                        "items",
                        "properties",
                        "enabled_builtin_kinds",
                    ],
                    builtin_description.clone(),
                );
            }
            Some("update_task") => {
                remove_tool_schema_property(
                    tool,
                    &["inputSchema", "properties", "patch", "properties"],
                    "status",
                );
            }
            _ => {}
        }
    }
}

fn planner_task_model_config_description(model_configs: &[ModelConfigRecord]) -> String {
    let mut lines = vec![
        "联系人异步任务必须指定模型配置 ID。请直接从当前启用模型中选择一个最合适的 default_model_config_id。"
            .to_string(),
    ];
    if model_configs.is_empty() {
        lines.push("当前还没有启用中的模型配置。".to_string());
        return lines.join("\n");
    }
    lines.push("当前启用模型：".to_string());
    for model in model_configs {
        lines.push(format!(
            "- {}: {} ({})。使用场景：{}",
            model.id,
            model.name,
            model.model,
            model
                .usage_scenario
                .as_deref()
                .filter(|value| !value.trim().is_empty())
                .unwrap_or("未填写")
        ));
    }
    lines.join("\n")
}

fn planner_builtin_mcp_kind_schema_description() -> String {
    let mut lines = vec![
        "联系人异步任务必须选择至少一个 builtin MCP 能力。只勾选本次执行真正需要的能力；不确定时可先调用 list_mcp_builtin_catalog 查看说明。"
            .to_string(),
    ];
    for value in mcp_builtin_kind_values() {
        if let Some(kind) = builtin_kind_by_any(value.as_str()) {
            let guide = mcp_builtin_kind_guide(kind);
            lines.push(format!(
                "- {}: {} 使用场景：{}。能力：{}。",
                value,
                guide.description,
                guide.use_cases.join("、"),
                guide.capabilities.join("、")
            ));
        }
    }
    lines.join("\n")
}

fn planner_schedule_contact_async_now(
    mut schedule: TaskScheduleConfig,
) -> Result<TaskScheduleConfig, String> {
    schedule.mode = TaskScheduleMode::ContactAsync;
    if schedule.interval_seconds.is_some() {
        schedule.interval_seconds = None;
    }
    let now = now_rfc3339();
    if schedule
        .run_at
        .as_deref()
        .is_none_or(|value| value.trim().is_empty())
    {
        schedule.run_at = Some(now.clone());
    }
    schedule.next_run_at = Some(now);
    Ok(schedule)
}
