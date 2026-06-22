use super::*;

pub(in crate::mcp_server) fn planner_update_task_request(
    patch: UpdateTaskRequest,
) -> Result<UpdateTaskRequest, String> {
    if patch.status.is_some() {
        return Err("联系人异步模式不能通过 update_task 修改任务执行状态".to_string());
    }
    Ok(patch)
}

pub(in crate::mcp_server) fn planner_root_create_request(
    mut input: CreateTaskRequest,
) -> Result<CreateTaskRequest, String> {
    ensure_planner_required_fields(&input)?;
    input.status = Some(TaskStatus::Ready);
    input.schedule = Some(planner_schedule_contact_async_now(
        input.schedule.unwrap_or_default(),
    )?);
    Ok(input)
}

pub(in crate::mcp_server) fn require_chatos_async_source_context(
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

pub(in crate::mcp_server) fn planner_prerequisite_create_request(
    mut input: CreateTaskRequest,
) -> Result<CreateTaskRequest, String> {
    ensure_planner_required_fields(&input)?;
    input.status = Some(TaskStatus::Ready);
    input.schedule = Some(planner_schedule_contact_async_now(
        input.schedule.unwrap_or_default(),
    )?);
    Ok(input)
}

pub(in crate::mcp_server) fn ensure_planner_required_fields(
    input: &CreateTaskRequest,
) -> Result<(), String> {
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
