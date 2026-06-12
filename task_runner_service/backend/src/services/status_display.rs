use crate::models::{TaskRunStatus, TaskScheduleMode, TaskStatus};

pub(super) trait TaskStatusExt {
    fn status_string(&self) -> &'static str;
}

impl TaskStatusExt for TaskStatus {
    fn status_string(&self) -> &'static str {
        match self {
            TaskStatus::Draft => "draft",
            TaskStatus::Ready => "ready",
            TaskStatus::Running => "running",
            TaskStatus::Succeeded => "succeeded",
            TaskStatus::Failed => "failed",
            TaskStatus::Blocked => "blocked",
            TaskStatus::Cancelled => "cancelled",
            TaskStatus::Archived => "archived",
        }
    }
}

impl TaskStatusExt for TaskRunStatus {
    fn status_string(&self) -> &'static str {
        match self {
            TaskRunStatus::Queued => "queued",
            TaskRunStatus::Running => "running",
            TaskRunStatus::Succeeded => "succeeded",
            TaskRunStatus::Failed => "failed",
            TaskRunStatus::Cancelled => "cancelled",
            TaskRunStatus::Blocked => "blocked",
        }
    }
}

pub(super) trait TaskScheduleModeExt {
    fn mode_key(&self) -> &'static str;
}

impl TaskScheduleModeExt for TaskScheduleMode {
    fn mode_key(&self) -> &'static str {
        match self {
            TaskScheduleMode::Manual => "manual",
            TaskScheduleMode::Once => "once",
            TaskScheduleMode::Interval => "interval",
            TaskScheduleMode::ContactAsync => "contact_async",
        }
    }
}
