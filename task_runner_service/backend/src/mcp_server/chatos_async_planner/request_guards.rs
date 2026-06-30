use super::*;

pub(in crate::mcp_server) fn planner_update_task_request(
    mut patch: UpdateTaskRequest,
) -> Result<UpdateTaskRequest, String> {
    if patch.status.is_some() {
        return Err("联系人异步模式不能通过 update_task 修改任务执行状态".to_string());
    }
    if let Some(config) = patch.mcp_config.as_mut() {
        ensure_system_injected_builtin_config(config);
    }
    Ok(patch)
}

pub(in crate::mcp_server) fn planner_root_create_request(
    mut input: CreateTaskRequest,
    request_context: &McpRequestContext,
) -> Result<CreateTaskRequest, String> {
    ensure_planner_required_fields(&input)?;
    ensure_system_injected_builtin_mcp(&mut input, request_context);
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
    request_context: &McpRequestContext,
) -> Result<CreateTaskRequest, String> {
    ensure_planner_required_fields(&input)?;
    ensure_system_injected_builtin_mcp(&mut input, request_context);
    input.status = Some(TaskStatus::Ready);
    input.schedule = Some(planner_schedule_contact_async_now(
        input.schedule.unwrap_or_default(),
    )?);
    Ok(input)
}

pub(in crate::mcp_server) fn ensure_planner_required_fields(
    input: &CreateTaskRequest,
) -> Result<(), String> {
    let _ = input;
    Ok(())
}

fn ensure_system_injected_builtin_mcp(
    input: &mut CreateTaskRequest,
    request_context: &McpRequestContext,
) {
    let defaults = TaskMcpConfig::default();
    let config = input.mcp_config.get_or_insert_with(|| TaskMcpConfig {
        enabled_builtin_kinds: Vec::new(),
        ..TaskMcpConfig::default()
    });
    config.enabled = true;
    config.init_mode = defaults.init_mode;
    config.builtin_prompt_mode = defaults.builtin_prompt_mode;
    config.builtin_prompt_locale = request_context.requested_builtin_prompt_locale();
    ensure_system_injected_builtin_config(config);
}

fn ensure_system_injected_builtin_config(config: &mut TaskMcpConfig) {
    let defaults = TaskMcpConfig::default();
    config.enabled = true;
    config.init_mode = defaults.init_mode;
    for kind in SYSTEM_INJECTED_BUILTIN_KINDS {
        if !config
            .enabled_builtin_kinds
            .iter()
            .any(|value| value.trim().eq_ignore_ascii_case(kind))
        {
            config.enabled_builtin_kinds.push((*kind).to_string());
        }
    }
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
