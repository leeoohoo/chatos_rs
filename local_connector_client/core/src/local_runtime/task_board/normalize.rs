// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_mcp::{TaskDraft, TaskOutcomeItem, TaskUpdatePatch};

pub(crate) fn normalize_task_draft(mut draft: TaskDraft) -> Result<TaskDraft, String> {
    draft.title = required_title(draft.title)?;
    draft.details = draft.details.trim().to_string();
    draft.priority = normalize_priority(draft.priority.as_str());
    draft.status = normalize_status(draft.status.as_str());
    draft.tags = normalize_list(draft.tags);
    draft.prerequisite_task_ids = normalize_prerequisites(
        draft.prerequisite_task_id.take(),
        draft.prerequisite_task_ids,
    );
    draft.due_at = normalize_optional(draft.due_at);
    draft.outcome_summary = draft.outcome_summary.trim().to_string();
    draft.outcome_items = normalize_outcomes(draft.outcome_items);
    draft.resume_hint = draft.resume_hint.trim().to_string();
    draft.blocker_reason = draft.blocker_reason.trim().to_string();
    draft.blocker_needs = normalize_list(draft.blocker_needs);
    draft.blocker_kind =
        normalize_blocker_kind(draft.blocker_kind.as_str(), draft.blocker_reason.as_str());
    validate_terminal_state(
        draft.status.as_str(),
        draft.outcome_summary.as_str(),
        draft.outcome_items.as_slice(),
        draft.blocker_reason.as_str(),
    )?;
    Ok(draft)
}

pub(crate) fn normalize_task_patch(mut patch: TaskUpdatePatch) -> Result<TaskUpdatePatch, String> {
    patch.title = patch.title.map(required_title).transpose()?;
    patch.details = patch.details.map(|value| value.trim().to_string());
    patch.priority = patch
        .priority
        .map(|value| normalize_priority(value.as_str()));
    patch.status = patch.status.map(|value| normalize_status(value.as_str()));
    patch.tags = patch.tags.map(normalize_list);
    patch.due_at = patch.due_at.map(normalize_optional);
    patch.outcome_summary = patch.outcome_summary.map(|value| value.trim().to_string());
    patch.outcome_items = patch.outcome_items.map(normalize_outcomes);
    patch.resume_hint = patch.resume_hint.map(|value| value.trim().to_string());
    patch.blocker_reason = patch.blocker_reason.map(|value| value.trim().to_string());
    patch.blocker_needs = patch.blocker_needs.map(normalize_list);
    patch.blocker_kind = patch
        .blocker_kind
        .map(|value| value.trim().to_ascii_lowercase());
    patch.completed_at = patch.completed_at.map(normalize_optional);
    patch.last_outcome_at = patch.last_outcome_at.map(normalize_optional);
    Ok(patch)
}

pub(crate) fn validate_terminal_state(
    status: &str,
    summary: &str,
    items: &[TaskOutcomeItem],
    blocker_reason: &str,
) -> Result<(), String> {
    if matches!(status, "done" | "blocked") && summary.trim().is_empty() && items.is_empty() {
        return Err(format!("{status} tasks must include an outcome"));
    }
    if status == "blocked" && blocker_reason.trim().is_empty() {
        return Err("blocked tasks must include blocker_reason".to_string());
    }
    Ok(())
}

fn required_title(value: String) -> Result<String, String> {
    let value = value.trim().to_string();
    (!value.is_empty())
        .then_some(value)
        .ok_or_else(|| "task title is required".to_string())
}

fn normalize_priority(value: &str) -> String {
    match value.trim().to_ascii_lowercase().as_str() {
        "high" => "high",
        "low" => "low",
        _ => "medium",
    }
    .to_string()
}

fn normalize_status(value: &str) -> String {
    match value.trim().to_ascii_lowercase().as_str() {
        "doing" => "doing",
        "blocked" => "blocked",
        "done" => "done",
        _ => "todo",
    }
    .to_string()
}

fn normalize_optional(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn normalize_list(values: Vec<String>) -> Vec<String> {
    values.into_iter().fold(Vec::new(), |mut result, value| {
        let value = value.trim().to_string();
        if !value.is_empty() && !result.contains(&value) {
            result.push(value);
        }
        result
    })
}

fn normalize_prerequisites(single: Option<String>, mut values: Vec<String>) -> Vec<String> {
    if let Some(single) = single {
        values.push(single);
    }
    normalize_list(values)
}

fn normalize_outcomes(values: Vec<TaskOutcomeItem>) -> Vec<TaskOutcomeItem> {
    values
        .into_iter()
        .filter_map(|mut item| {
            item.kind = item.kind.trim().to_ascii_lowercase();
            item.text = item.text.trim().to_string();
            item.refs = normalize_list(item.refs);
            (!item.text.is_empty()).then_some(item)
        })
        .collect()
}

fn normalize_blocker_kind(value: &str, reason: &str) -> String {
    match value.trim().to_ascii_lowercase().as_str() {
        "external_dependency"
        | "permission"
        | "missing_information"
        | "design_decision"
        | "environment_failure"
        | "upstream_bug" => value.trim().to_ascii_lowercase(),
        _ if reason.trim().is_empty() => String::new(),
        _ => "unknown".to_string(),
    }
}
