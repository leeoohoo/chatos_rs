// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::services::task_manager::types::{TaskRecord, TaskUpdatePatch};

pub(super) fn merged_task_record(mut task: TaskRecord, patch: &TaskUpdatePatch) -> TaskRecord {
    if let Some(value) = patch.title.as_ref() {
        task.title = value.clone();
    }
    if let Some(value) = patch.details.as_ref() {
        task.details = value.clone();
    }
    if let Some(value) = patch.priority.as_ref() {
        task.priority = value.clone();
    }
    if let Some(value) = patch.status.as_ref() {
        task.status = value.clone();
    }
    if let Some(values) = patch.tags.as_ref() {
        task.tags = values.clone();
    }
    if let Some(value) = patch.due_at.as_ref() {
        task.due_at = value.clone();
    }
    if let Some(value) = patch.outcome_summary.as_ref() {
        task.outcome_summary = value.clone();
    }
    if let Some(values) = patch.outcome_items.as_ref() {
        task.outcome_items = values.clone();
    }
    if let Some(value) = patch.resume_hint.as_ref() {
        task.resume_hint = value.clone();
    }
    if let Some(value) = patch.blocker_reason.as_ref() {
        task.blocker_reason = value.clone();
    }
    if let Some(values) = patch.blocker_needs.as_ref() {
        task.blocker_needs = values.clone();
    }
    if let Some(value) = patch.blocker_kind.as_ref() {
        task.blocker_kind = value.clone();
    }
    if let Some(value) = patch.completed_at.as_ref() {
        task.completed_at = value.clone();
    }
    if let Some(value) = patch.last_outcome_at.as_ref() {
        task.last_outcome_at = value.clone();
    }
    task
}

pub(super) fn validate_terminal_task_state(task: &TaskRecord) -> Result<(), String> {
    match task.status.as_str() {
        "done" => {
            if !task_has_outcome(task) {
                return Err(
                    "done tasks must include outcome_summary or outcome_items so later tasks can reuse the result".to_string(),
                );
            }
        }
        "blocked" => {
            if !task_has_outcome(task) {
                return Err(
                    "blocked tasks must include outcome_summary or outcome_items to record what was already tried".to_string(),
                );
            }
            if task.blocker_reason.trim().is_empty() {
                return Err(
                    "blocked tasks must include blocker_reason so the next task knows why progress stopped".to_string(),
                );
            }
        }
        _ => {}
    }

    Ok(())
}

pub(super) fn apply_terminal_state_defaults(patch: &mut TaskUpdatePatch) {
    let has_outcome = patch
        .outcome_summary
        .as_deref()
        .map(str::trim)
        .map(|value| !value.is_empty())
        .unwrap_or(false)
        || patch
            .outcome_items
            .as_ref()
            .map(|items| !items.is_empty())
            .unwrap_or(false);
    let next_status = patch.status.as_deref().unwrap_or_default();

    match next_status {
        "done" => {
            if patch.completed_at.is_none() {
                patch.completed_at = Some(Some(crate::core::time::now_rfc3339()));
            }
            if patch.last_outcome_at.is_none() && has_outcome {
                patch.last_outcome_at = Some(Some(crate::core::time::now_rfc3339()));
            }
        }
        "blocked" => {
            if patch.completed_at.is_none() {
                patch.completed_at = Some(None);
            }
            if patch.last_outcome_at.is_none() && has_outcome {
                patch.last_outcome_at = Some(Some(crate::core::time::now_rfc3339()));
            }
        }
        _ => {}
    }
}

fn task_has_outcome(task: &TaskRecord) -> bool {
    !task.outcome_summary.trim().is_empty() || !task.outcome_items.is_empty()
}
