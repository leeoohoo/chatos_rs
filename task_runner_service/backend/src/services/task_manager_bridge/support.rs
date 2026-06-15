use super::*;

pub(super) fn task_belongs_to_context(task: &TaskRecord, root_task_id: &str) -> bool {
    task.id == root_task_id || task.parent_task_id.as_deref() == Some(root_task_id)
}

pub(super) fn task_to_manager_value(task: &TaskRecord) -> Value {
    json!({
        "id": task.id.clone(),
        "parent_task_id": task.parent_task_id.clone(),
        "source_run_id": task.source_run_id.clone(),
        "title": task.title.clone(),
        "details": task
            .description
            .clone()
            .or_else(|| normalized_optional(Some(task.objective.clone()))),
        "priority": task_priority_to_manager_label(task.priority),
        "status": task_manager_status_from_task_status(task.status),
        "tags": task.tags.clone(),
        "due_at": task.task_tool_state.due_at.clone(),
        "outcome_summary": task.result_summary.clone(),
        "outcome_items": tool_state_outcomes_into_shared(&task.task_tool_state.outcome_items),
        "resume_hint": task.task_tool_state.resume_hint.clone(),
        "blocker_reason": task.task_tool_state.blocker_reason.clone(),
        "blocker_needs": task.task_tool_state.blocker_needs.clone(),
        "blocker_kind": task.task_tool_state.blocker_kind.clone(),
        "completed_at": task.task_tool_state.completed_at.clone(),
        "last_outcome_at": task.task_tool_state.last_outcome_at.clone(),
        "created_at": task.created_at.clone(),
        "updated_at": task.updated_at.clone(),
    })
}

pub(super) fn apply_manager_patch(
    task: &mut TaskRecord,
    patch: SharedTaskUpdatePatch,
    mark_complete: bool,
    now: &str,
) -> Result<(), String> {
    let requested_status = patch.status.as_deref().map(task_status_from_manager_status);
    if let Some(title) = patch.title {
        validate_required("title", &title)?;
        task.title = title.trim().to_string();
    }
    if let Some(details) = patch.details {
        let normalized = normalized_optional(Some(details));
        task.description = normalized.clone();
        if task.parent_task_id.is_some() {
            task.objective = normalized.unwrap_or_else(|| task.title.clone());
        }
    }
    if let Some(priority) = patch.priority {
        task.priority = task_priority_from_manager_label(priority.as_str());
    }
    if let Some(status) = requested_status {
        task.status = status;
    }
    if let Some(tags) = patch.tags {
        task.tags = normalize_strings(tags);
    }
    if let Some(due_at) = patch.due_at {
        task.task_tool_state.due_at = normalized_optional_nested(due_at);
    }
    if let Some(outcome_summary) = patch.outcome_summary {
        task.result_summary = normalized_optional(Some(outcome_summary));
        if task.result_summary.is_some() {
            task.task_tool_state.last_outcome_at = Some(now.to_string());
        }
    }
    if let Some(outcome_items) = patch.outcome_items {
        task.task_tool_state.outcome_items = shared_outcome_items_into_tool_state(outcome_items);
        if !task.task_tool_state.outcome_items.is_empty() {
            task.task_tool_state.last_outcome_at = Some(now.to_string());
        }
    }
    if let Some(resume_hint) = patch.resume_hint {
        task.task_tool_state.resume_hint = normalized_optional(Some(resume_hint));
    }
    if let Some(blocker_reason) = patch.blocker_reason {
        task.task_tool_state.blocker_reason = normalized_optional(Some(blocker_reason));
    }
    if let Some(blocker_needs) = patch.blocker_needs {
        task.task_tool_state.blocker_needs = normalize_strings(blocker_needs);
    }
    if let Some(blocker_kind) = patch.blocker_kind {
        task.task_tool_state.blocker_kind = normalized_optional(Some(blocker_kind));
    }
    if let Some(completed_at) = patch.completed_at {
        task.task_tool_state.completed_at = normalized_optional_nested(completed_at);
    }
    if let Some(last_outcome_at) = patch.last_outcome_at {
        task.task_tool_state.last_outcome_at = normalized_optional_nested(last_outcome_at);
    }
    if mark_complete || matches!(task.status, TaskStatus::Succeeded) {
        task.status = TaskStatus::Succeeded;
        if task.task_tool_state.completed_at.is_none() {
            task.task_tool_state.completed_at = Some(now.to_string());
        }
        if task.task_tool_state.last_outcome_at.is_none() {
            task.task_tool_state.last_outcome_at = Some(now.to_string());
        }
    }
    Ok(())
}

pub(super) fn task_status_from_manager_status(value: &str) -> TaskStatus {
    match value.trim().to_ascii_lowercase().as_str() {
        "doing" => TaskStatus::Running,
        "blocked" => TaskStatus::Blocked,
        "done" => TaskStatus::Succeeded,
        _ => TaskStatus::Ready,
    }
}

pub(super) fn task_manager_status_from_task_status(status: TaskStatus) -> &'static str {
    match status {
        TaskStatus::Running => "doing",
        TaskStatus::Blocked | TaskStatus::Failed => "blocked",
        TaskStatus::Succeeded | TaskStatus::Cancelled | TaskStatus::Archived => "done",
        TaskStatus::Draft | TaskStatus::Ready => "todo",
    }
}

pub(super) fn task_priority_from_manager_label(value: &str) -> i32 {
    match value.trim().to_ascii_lowercase().as_str() {
        "high" => 100,
        "low" => 10,
        _ => 50,
    }
}

pub(super) fn task_priority_to_manager_label(value: i32) -> &'static str {
    if value >= 80 {
        "high"
    } else if value <= 20 {
        "low"
    } else {
        "medium"
    }
}

pub(super) fn shared_outcome_items_into_tool_state(
    items: Vec<SharedTaskOutcomeItem>,
) -> Vec<TaskToolOutcomeItem> {
    items
        .into_iter()
        .map(|item| TaskToolOutcomeItem {
            kind: item.kind,
            text: item.text,
            importance: item.importance,
            refs: item.refs,
        })
        .collect()
}

pub(super) fn tool_state_outcomes_into_shared(
    items: &[TaskToolOutcomeItem],
) -> Vec<SharedTaskOutcomeItem> {
    items
        .iter()
        .map(|item| SharedTaskOutcomeItem {
            kind: item.kind.clone(),
            text: item.text.clone(),
            importance: item.importance.clone(),
            refs: item.refs.clone(),
        })
        .collect()
}
