// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

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
    _request_context: &McpRequestContext,
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
    _request_context: &McpRequestContext,
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
    let mcp_config = input.mcp_config.as_ref().ok_or_else(|| {
        "创建任务时必须显式填写 requires_execution 和 enabled_builtin_kinds；由 Agent 根据任务目标选择最小工具能力"
            .to_string()
    })?;
    if input
        .task_profile
        .as_deref()
        .is_some_and(chatos_agent::is_chatos_plan_task_profile)
        && mcp_config.enabled_builtin_kinds.iter().any(|kind| {
            matches!(
                chatos_mcp_runtime::builtin_kind_by_any(kind),
                Some(
                    chatos_mcp_runtime::BuiltinMcpKind::CodeMaintainerWrite
                        | chatos_mcp_runtime::BuiltinMcpKind::TerminalController
                )
            )
        })
    {
        return Err("规划任务只能选择只读项目能力，不能选择文件写入或终端执行能力".to_string());
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
