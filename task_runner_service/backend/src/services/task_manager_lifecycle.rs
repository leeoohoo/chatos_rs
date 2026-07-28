// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashMap;

use serde_json::{json, Value};
use tracing::{info, warn};

use super::*;

#[derive(Debug, Clone, Default)]
pub(super) struct TaskSessionSnapshot {
    pub(super) entries: Vec<TaskRecord>,
    pub(super) open_required: Vec<TaskRecord>,
    pub(super) open_optional: Vec<TaskRecord>,
    pub(super) terminal_blocked: Vec<TaskRecord>,
}

impl TaskSessionSnapshot {
    pub(super) fn progress_signature(&self) -> String {
        let mut parts = self
            .open_required
            .iter()
            .map(|task| {
                format!(
                    "{}:{:?}:{:?}:{}",
                    task.id,
                    task.status,
                    effective_task_closure_state(task),
                    task.updated_at
                )
            })
            .collect::<Vec<_>>();
        parts.sort();
        parts.join("|")
    }
}

#[derive(Debug, Clone, Default)]
pub(super) struct TaskSessionFinalizeSummary {
    pub(super) satisfied: usize,
    pub(super) waived: usize,
    pub(super) blocked_terminal: usize,
    pub(super) cancelled: usize,
    pub(super) orphaned: usize,
    pub(super) durable_detached: usize,
}

impl TaskSessionFinalizeSummary {
    pub(super) fn total_changed(&self) -> usize {
        self.satisfied + self.waived + self.cancelled + self.orphaned + self.durable_detached
    }

    pub(super) fn payload(&self, session_id: &str, terminal_status: TaskRunStatus) -> Value {
        json!({
            "session_id": session_id,
            "terminal_status": terminal_status,
            "satisfied": self.satisfied,
            "waived": self.waived,
            "blocked_terminal": self.blocked_terminal,
            "cancelled": self.cancelled,
            "orphaned": self.orphaned,
            "durable_detached": self.durable_detached,
        })
    }
}

pub(super) fn effective_task_manager_scope(task: &TaskRecord) -> TaskManagerScope {
    task.task_tool_state.manager_scope.unwrap_or_else(|| {
        if task.parent_task_id.is_some() && task.source_run_id.is_some() {
            TaskManagerScope::RunChecklist
        } else {
            TaskManagerScope::DurableFollowup
        }
    })
}

pub(super) fn task_has_manager_lifecycle(task: &TaskRecord) -> bool {
    task.parent_task_id.is_some()
        && (task.task_tool_state.manager_scope.is_some()
            || task.task_tool_state.task_session_id.is_some()
            || task.source_run_id.is_some())
}

pub(super) fn effective_task_closure_state(task: &TaskRecord) -> TaskClosureState {
    if task.status == TaskStatus::Succeeded {
        return TaskClosureState::Satisfied;
    }
    task.task_tool_state
        .closure_state
        .unwrap_or(match task.status {
            TaskStatus::Succeeded => TaskClosureState::Satisfied,
            TaskStatus::Cancelled => TaskClosureState::Cancelled,
            TaskStatus::Archived => TaskClosureState::Superseded,
            TaskStatus::Draft
            | TaskStatus::Ready
            | TaskStatus::Queued
            | TaskStatus::Running
            | TaskStatus::Failed
            | TaskStatus::Blocked => TaskClosureState::Open,
        })
}

pub(super) fn effective_task_required_for_parent_completion(task: &TaskRecord) -> bool {
    match task.task_tool_state.manager_scope {
        Some(TaskManagerScope::DurableFollowup) => return false,
        Some(TaskManagerScope::RunChecklist) => {}
        None if task.parent_task_id.is_some() && task.source_run_id.is_some() => {}
        None => {
            return task
                .task_tool_state
                .required_for_parent_completion
                .unwrap_or(true);
        }
    }
    task.task_tool_state
        .required_for_parent_completion
        .unwrap_or(true)
}

pub(super) fn effective_task_session_id(task: &TaskRecord) -> Option<&str> {
    task.task_tool_state.task_session_id.as_deref().or_else(|| {
        (effective_task_manager_scope(task) == TaskManagerScope::RunChecklist)
            .then_some(task.source_run_id.as_deref())
            .flatten()
    })
}

pub(super) fn task_is_in_session(task: &TaskRecord, session_id: &str) -> bool {
    effective_task_session_id(task) == Some(session_id)
}

pub(super) fn task_closure_is_terminal(task: &TaskRecord) -> bool {
    effective_task_closure_state(task) != TaskClosureState::Open
}

pub(super) fn task_blocks_parent_completion(task: &TaskRecord, session_id: &str) -> bool {
    task_is_in_session(task, session_id)
        && effective_task_required_for_parent_completion(task)
        && effective_task_closure_state(task) == TaskClosureState::Open
}

pub(super) fn task_terminally_blocks_parent(task: &TaskRecord, session_id: &str) -> bool {
    task_is_in_session(task, session_id)
        && effective_task_required_for_parent_completion(task)
        && effective_task_closure_state(task) == TaskClosureState::BlockedTerminal
}

pub(super) async fn load_task_session_snapshot(
    store: &AppStore,
    root_task_id: &str,
    session_id: &str,
) -> Result<TaskSessionSnapshot, String> {
    let mut entries = store
        .list_tasks_filtered(&TaskListFilters {
            parent_task_id: Some(root_task_id.to_string()),
            ..TaskListFilters::default()
        })
        .await?
        .into_iter()
        .filter(|task| task_is_in_session(task, session_id))
        .collect::<Vec<_>>();
    entries.sort_by(|left, right| right.updated_at.cmp(&left.updated_at));

    let open_required = entries
        .iter()
        .filter(|task| task_blocks_parent_completion(task, session_id))
        .cloned()
        .collect::<Vec<_>>();
    let open_optional = entries
        .iter()
        .filter(|task| {
            effective_task_closure_state(task) == TaskClosureState::Open
                && !effective_task_required_for_parent_completion(task)
        })
        .cloned()
        .collect::<Vec<_>>();
    let terminal_blocked = entries
        .iter()
        .filter(|task| task_terminally_blocks_parent(task, session_id))
        .cloned()
        .collect::<Vec<_>>();

    Ok(TaskSessionSnapshot {
        entries,
        open_required,
        open_optional,
        terminal_blocked,
    })
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

pub(super) async fn block_open_required_task_session_entries(
    store: &AppStore,
    root_task_id: &str,
    session_id: &str,
    reason: &str,
) -> Result<Vec<TaskRecord>, String> {
    let snapshot = load_task_session_snapshot(store, root_task_id, session_id).await?;
    let mut changed = Vec::new();
    for mut task in snapshot.entries.into_iter().filter(|task| {
        effective_task_closure_state(task) == TaskClosureState::Open
            && effective_task_required_for_parent_completion(task)
            && effective_task_manager_scope(task) == TaskManagerScope::RunChecklist
    }) {
        let now = now_rfc3339();
        apply_task_closure(
            &mut task,
            TaskClosureState::BlockedTerminal,
            Some(reason.to_string()),
            now.as_str(),
        )?;
        changed.push(store.save_task(task).await?);
    }
    Ok(changed)
}

pub(super) async fn finalize_task_session_entries(
    store: &AppStore,
    root_task_id: &str,
    session_id: &str,
    terminal_status: TaskRunStatus,
) -> Result<TaskSessionFinalizeSummary, String> {
    let snapshot = load_task_session_snapshot(store, root_task_id, session_id).await?;
    let mut summary = TaskSessionFinalizeSummary {
        blocked_terminal: snapshot.terminal_blocked.len(),
        ..TaskSessionFinalizeSummary::default()
    };

    for mut task in snapshot.entries {
        let closure_state = effective_task_closure_state(&task);
        let is_durable = effective_task_manager_scope(&task) == TaskManagerScope::DurableFollowup;
        let mut task_changed = false;
        if closure_state == TaskClosureState::Satisfied
            && task.task_tool_state.closure_state != Some(TaskClosureState::Satisfied)
        {
            let now = now_rfc3339();
            apply_task_closure(&mut task, TaskClosureState::Satisfied, None, now.as_str())?;
            summary.satisfied += 1;
            task_changed = true;
        }

        if is_durable {
            if task.task_tool_state.task_session_id.is_some()
                || task.task_tool_state.required_for_parent_completion != Some(false)
            {
                let now = now_rfc3339();
                task.task_tool_state.task_session_id = None;
                task.task_tool_state.required_for_parent_completion = Some(false);
                task.task_tool_state.lifecycle_updated_at = Some(now.clone());
                task.updated_at = now;
                task_changed = true;
                summary.durable_detached += 1;
            }
            if task_changed {
                store.save_task(task).await?;
            }
            continue;
        }

        if task_changed {
            store.save_task(task).await?;
            continue;
        }

        if closure_state != TaskClosureState::Open {
            let replacement = match (terminal_status, closure_state) {
                (TaskRunStatus::Cancelled, TaskClosureState::BlockedTerminal) => {
                    Some(TaskClosureState::Cancelled)
                }
                (TaskRunStatus::Failed, TaskClosureState::BlockedTerminal) => {
                    Some(TaskClosureState::Orphaned)
                }
                _ => None,
            };
            let Some(replacement) = replacement else {
                continue;
            };
            let reason = match replacement {
                TaskClosureState::Cancelled => "父运行已取消；终态阻塞清单随运行一起取消",
                TaskClosureState::Orphaned => {
                    "父运行失败；终态阻塞清单已标记为孤立，等待显式重试接管"
                }
                _ => unreachable!(),
            };
            let now = now_rfc3339();
            apply_task_closure(
                &mut task,
                replacement,
                Some(reason.to_string()),
                now.as_str(),
            )?;
            store.save_task(task).await?;
            match replacement {
                TaskClosureState::Cancelled => summary.cancelled += 1,
                TaskClosureState::Orphaned => summary.orphaned += 1,
                _ => {}
            }
            continue;
        }

        let (next_state, reason) = match terminal_status {
            TaskRunStatus::Succeeded => (
                TaskClosureState::Waived,
                "父运行已成功结束；该运行内清单未显式关闭，系统已保留审计记录并自动豁免",
            ),
            TaskRunStatus::Cancelled => (
                TaskClosureState::Cancelled,
                "父运行已取消；该运行内清单随运行一起取消",
            ),
            TaskRunStatus::Failed => (
                TaskClosureState::Orphaned,
                "父运行失败；该运行内清单已标记为孤立，不再污染后续重试",
            ),
            TaskRunStatus::Blocked => (
                TaskClosureState::Orphaned,
                "父运行进入阻塞终态；其余未关闭清单已标记为孤立",
            ),
            TaskRunStatus::Queued | TaskRunStatus::Running => continue,
        };
        let now = now_rfc3339();
        apply_task_closure(
            &mut task,
            next_state,
            Some(reason.to_string()),
            now.as_str(),
        )?;
        store.save_task(task).await?;
        match next_state {
            TaskClosureState::Waived => summary.waived += 1,
            TaskClosureState::Cancelled => summary.cancelled += 1,
            TaskClosureState::Orphaned => summary.orphaned += 1,
            _ => {}
        }
    }
    Ok(summary)
}

pub(super) async fn migrate_legacy_task_manager_entries(store: &AppStore) -> Result<usize, String> {
    let tasks = store.list_tasks().await?;
    let candidates = tasks
        .into_iter()
        .filter(|task| {
            task.parent_task_id.is_some()
                && task.source_run_id.is_some()
                && task.task_tool_state.closure_state.is_none()
        })
        .collect::<Vec<_>>();
    if candidates.is_empty() {
        return Ok(0);
    }

    let mut run_cache = HashMap::<String, Option<TaskRunRecord>>::new();
    let mut migrated = 0usize;
    for mut task in candidates {
        let source_run_id = task.source_run_id.clone().unwrap_or_default();
        let run = if let Some(cached) = run_cache.get(source_run_id.as_str()) {
            cached.clone()
        } else {
            let loaded = store.get_run(source_run_id.as_str()).await?;
            run_cache.insert(source_run_id.clone(), loaded.clone());
            loaded
        };
        task.task_tool_state.manager_scope = Some(TaskManagerScope::RunChecklist);
        task.task_tool_state.task_session_id = Some(source_run_id.clone());
        task.task_tool_state.required_for_parent_completion = Some(true);

        let now = now_rfc3339();
        if task.status == TaskStatus::Succeeded {
            apply_task_closure(&mut task, TaskClosureState::Satisfied, None, now.as_str())?;
        } else if run
            .as_ref()
            .is_some_and(|run| matches!(run.status, TaskRunStatus::Queued | TaskRunStatus::Running))
        {
            task.task_tool_state.closure_state = Some(TaskClosureState::Open);
            task.task_tool_state.lifecycle_updated_at = Some(now.clone());
            task.updated_at = now;
        } else {
            let closure_state = if run
                .as_ref()
                .is_some_and(|run| run.status == TaskRunStatus::Cancelled)
            {
                TaskClosureState::Cancelled
            } else {
                TaskClosureState::Orphaned
            };
            apply_task_closure(
                &mut task,
                closure_state,
                Some("历史 Task Manager 子任务所属运行已终止，迁移后不再阻塞未来运行".to_string()),
                now.as_str(),
            )?;
        }
        store.save_task(task).await?;
        migrated += 1;
    }
    info!(migrated, "migrated legacy task manager lifecycle entries");
    Ok(migrated)
}

pub(super) async fn append_task_session_finalized_event(
    store: &AppStore,
    run: &TaskRunRecord,
    summary: &TaskSessionFinalizeSummary,
) {
    if summary.total_changed() == 0 && summary.blocked_terminal == 0 {
        return;
    }
    if let Err(err) = store
        .append_run_event(TaskRunEventRecord::new(
            run.id.clone(),
            "task_session_finalized",
            Some("Task Manager 运行级任务会话已完成程序化收口".to_string()),
            Some(summary.payload(run.id.as_str(), run.status)),
        ))
        .await
    {
        warn!(
            run_id = run.id.as_str(),
            error = err.as_str(),
            "failed to append task session finalized event"
        );
    }
}
