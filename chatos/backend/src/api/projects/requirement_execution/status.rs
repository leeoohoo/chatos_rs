// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

pub(super) fn project_work_item_status_is_active(status: &str) -> bool {
    matches!(
        chatos_project_execution::classify_execution_task_status(status),
        chatos_project_execution::ExecutionTaskState::Active
    )
}

pub(in crate::api::projects) fn task_runner_status_is_active(status: Option<&str>) -> bool {
    chatos_project_execution::execution_task_status_is_active(status.unwrap_or_default())
}

pub(in crate::api::projects) fn task_runner_status_is_success(status: Option<&str>) -> bool {
    chatos_project_execution::execution_task_status_is_success(status.unwrap_or_default())
}

pub(in crate::api::projects) fn task_runner_status_is_cancelled(status: Option<&str>) -> bool {
    matches!(
        status
            .unwrap_or_default()
            .trim()
            .to_ascii_lowercase()
            .as_str(),
        "cancelled" | "canceled"
    )
}

pub(in crate::api::projects) fn task_runner_callback_event_for_status(
    status: &str,
) -> Option<&'static str> {
    match status.trim().to_ascii_lowercase().as_str() {
        "cancelled" | "canceled" => Some("task.cancelled"),
        "succeeded" | "success" | "completed" | "done" => Some("task.completed"),
        "failed" | "error" => Some("task.failed"),
        "blocked" => Some("task.blocked"),
        _ => None,
    }
}
