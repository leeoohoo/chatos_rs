pub(super) fn project_work_item_status_is_active(status: &str) -> bool {
    matches!(
        status.trim().to_ascii_lowercase().as_str(),
        "queued" | "running" | "processing" | "in_progress" | "pending"
    )
}

pub(in crate::api::projects) fn task_runner_status_is_active(status: Option<&str>) -> bool {
    matches!(
        status
            .unwrap_or_default()
            .trim()
            .to_ascii_lowercase()
            .as_str(),
        "ready" | "queued" | "running" | "processing" | "in_progress" | "pending"
    )
}

pub(in crate::api::projects) fn task_runner_status_is_success(status: Option<&str>) -> bool {
    matches!(
        status
            .unwrap_or_default()
            .trim()
            .to_ascii_lowercase()
            .as_str(),
        "succeeded" | "success" | "completed" | "done"
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

pub(in crate::api::projects) fn is_done_status(status: &str) -> bool {
    matches!(
        status.trim().to_ascii_lowercase().as_str(),
        "done" | "succeeded" | "success" | "completed"
    )
}
