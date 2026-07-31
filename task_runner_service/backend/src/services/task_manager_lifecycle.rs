// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

pub(super) fn task_has_manager_lifecycle(task: &TaskRecord) -> bool {
    task.parent_task_id.is_some()
        && (task.task_tool_state.manager_scope.is_some()
            || task.task_tool_state.task_session_id.is_some()
            || task.source_run_id.is_some())
}

pub(super) fn apply_task_closure(
    task: &mut TaskRecord,
    closure_state: TaskClosureState,
    reason: Option<String>,
    now: &str,
) -> Result<(), String> {
    let reason = normalized_optional(reason);
    if closure_state != TaskClosureState::Satisfied && reason.is_none() {
        return Err(format!(
            "closing task {} as {:?} requires a reason",
            task.id, closure_state
        ));
    }

    task.task_tool_state.closure_state = Some(closure_state);
    task.task_tool_state.closure_reason = reason.clone();
    task.task_tool_state.lifecycle_updated_at = Some(now.to_string());
    task.task_tool_state.completed_at = Some(now.to_string());
    match closure_state {
        TaskClosureState::Open => {
            task.task_tool_state.completed_at = None;
        }
        TaskClosureState::Satisfied => {
            task.status = TaskStatus::Succeeded;
            task.task_tool_state.blocker_reason = None;
            task.task_tool_state.blocker_needs.clear();
            task.task_tool_state.blocker_kind = None;
            task.task_tool_state.closure_reason = None;
        }
        TaskClosureState::BlockedTerminal => {
            task.status = TaskStatus::Blocked;
            if task.task_tool_state.blocker_reason.is_none() {
                task.task_tool_state.blocker_reason = reason;
            }
        }
        TaskClosureState::Cancelled | TaskClosureState::Orphaned => {
            task.status = TaskStatus::Cancelled;
            task.task_tool_state.cancel_reason = reason;
            task.task_tool_state.cancelled_at = Some(now.to_string());
        }
        TaskClosureState::Superseded | TaskClosureState::Waived => {
            task.status = TaskStatus::Archived;
        }
    }
    task.updated_at = now.to_string();
    Ok(())
}
