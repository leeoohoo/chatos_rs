// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::models::{AskUserPromptStatus, TaskRunStatus, TaskStatus};

pub(super) fn task_status_to_str(status: TaskStatus) -> &'static str {
    match status {
        TaskStatus::Draft => "draft",
        TaskStatus::Ready => "ready",
        TaskStatus::Queued => "queued",
        TaskStatus::Running => "running",
        TaskStatus::Succeeded => "succeeded",
        TaskStatus::Failed => "failed",
        TaskStatus::Blocked => "blocked",
        TaskStatus::Cancelled => "cancelled",
        TaskStatus::Archived => "archived",
    }
}

pub(super) fn task_run_status_to_str(status: TaskRunStatus) -> &'static str {
    match status {
        TaskRunStatus::Queued => "queued",
        TaskRunStatus::Running => "running",
        TaskRunStatus::Succeeded => "succeeded",
        TaskRunStatus::Failed => "failed",
        TaskRunStatus::Cancelled => "cancelled",
        TaskRunStatus::Blocked => "blocked",
    }
}

pub(super) fn ask_user_prompt_status_to_str(status: AskUserPromptStatus) -> &'static str {
    match status {
        AskUserPromptStatus::Pending => "pending",
        AskUserPromptStatus::Submitted => "submitted",
        AskUserPromptStatus::Cancelled => "cancelled",
        AskUserPromptStatus::TimedOut => "timed_out",
        AskUserPromptStatus::Failed => "failed",
    }
}
