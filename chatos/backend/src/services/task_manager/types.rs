// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde::{Deserialize, Serialize};

use super::normalizer::{normalize_priority, normalize_status, normalize_tags, trimmed_non_empty};

#[cfg(test)]
pub const REVIEW_TIMEOUT_MS_DEFAULT: u64 = 86_400_000;
#[cfg(test)]
pub const REVIEW_TIMEOUT_ERR: &str = "review_timeout";
#[cfg(test)]
pub const REVIEW_NOT_FOUND_ERR: &str = "review_not_found";
pub const TASK_NOT_FOUND_ERR: &str = "task_not_found";

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TaskOutcomeItem {
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub importance: Option<String>,
    #[serde(default)]
    pub refs: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskDraft {
    pub title: String,
    #[serde(default)]
    pub details: String,
    #[serde(default = "default_priority")]
    pub priority: String,
    #[serde(default = "default_status")]
    pub status: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub due_at: Option<String>,
    #[serde(default)]
    pub outcome_summary: String,
    #[serde(default)]
    pub outcome_items: Vec<TaskOutcomeItem>,
    #[serde(default)]
    pub resume_hint: String,
    #[serde(default)]
    pub blocker_reason: String,
    #[serde(default)]
    pub blocker_needs: Vec<String>,
    #[serde(default)]
    pub blocker_kind: String,
}

pub type TaskUpdatePatch = chatos_mcp_runtime::TaskUpdatePatch<TaskOutcomeItem>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskRecord {
    pub id: String,
    #[serde(rename = "conversation_id")]
    pub conversation_id: String,
    pub conversation_turn_id: String,
    pub title: String,
    pub details: String,
    pub priority: String,
    pub status: String,
    pub tags: Vec<String>,
    pub due_at: Option<String>,
    pub outcome_summary: String,
    pub outcome_items: Vec<TaskOutcomeItem>,
    pub resume_hint: String,
    pub blocker_reason: String,
    pub blocker_needs: Vec<String>,
    pub blocker_kind: String,
    pub completed_at: Option<String>,
    pub last_outcome_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[cfg(test)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskCreateReviewPayload {
    pub review_id: String,
    #[serde(rename = "conversation_id")]
    pub conversation_id: String,
    pub conversation_turn_id: String,
    pub draft_tasks: Vec<TaskDraft>,
    pub timeout_ms: u64,
}

#[cfg(test)]
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum TaskReviewAction {
    Confirm,
    Cancel,
}

#[cfg(test)]
#[derive(Debug, Clone)]
pub struct TaskReviewDecision {
    pub action: TaskReviewAction,
    pub tasks: Vec<TaskDraft>,
    pub reason: Option<String>,
}

pub(super) fn task_update_patch_is_empty(patch: &TaskUpdatePatch) -> bool {
    patch.title.is_none()
        && patch.details.is_none()
        && patch.priority.is_none()
        && patch.status.is_none()
        && patch.tags.is_none()
        && patch.due_at.is_none()
        && patch.outcome_summary.is_none()
        && patch.outcome_items.is_none()
        && patch.resume_hint.is_none()
        && patch.blocker_reason.is_none()
        && patch.blocker_needs.is_none()
        && patch.blocker_kind.is_none()
        && patch.completed_at.is_none()
        && patch.last_outcome_at.is_none()
}

pub(super) fn normalize_task_update_patch(
    mut patch: TaskUpdatePatch,
) -> Result<TaskUpdatePatch, String> {
    if let Some(title) = patch.title.take() {
        let title = title.trim().to_string();
        if title.is_empty() {
            return Err("task title is required".to_string());
        }
        patch.title = Some(title);
    }

    if let Some(details) = patch.details.take() {
        patch.details = Some(details.trim().to_string());
    }

    if let Some(priority) = patch.priority.take() {
        patch.priority = Some(normalize_priority(priority.as_str()));
    }

    if let Some(status) = patch.status.take() {
        patch.status = Some(normalize_status(status.as_str()));
    }

    if let Some(tags) = patch.tags.take() {
        patch.tags = Some(normalize_tags(tags));
    }

    if let Some(due_at) = patch.due_at.take() {
        let normalized = due_at
            .as_deref()
            .and_then(trimmed_non_empty)
            .map(|value| value.to_string());
        patch.due_at = Some(normalized);
    }

    if let Some(outcome_summary) = patch.outcome_summary.take() {
        patch.outcome_summary = Some(outcome_summary.trim().to_string());
    }

    if let Some(outcome_items) = patch.outcome_items.take() {
        patch.outcome_items = Some(normalize_outcome_items(outcome_items));
    }

    if let Some(resume_hint) = patch.resume_hint.take() {
        patch.resume_hint = Some(resume_hint.trim().to_string());
    }

    if let Some(blocker_reason) = patch.blocker_reason.take() {
        patch.blocker_reason = Some(blocker_reason.trim().to_string());
    }

    if let Some(blocker_needs) = patch.blocker_needs.take() {
        patch.blocker_needs = Some(normalize_string_list(blocker_needs));
    }

    if let Some(blocker_kind) = patch.blocker_kind.take() {
        patch.blocker_kind = Some(normalize_blocker_kind(blocker_kind.as_str()));
    }

    if let Some(completed_at) = patch.completed_at.take() {
        let normalized = completed_at
            .as_deref()
            .and_then(trimmed_non_empty)
            .map(|value| value.to_string());
        patch.completed_at = Some(normalized);
    }

    if let Some(last_outcome_at) = patch.last_outcome_at.take() {
        let normalized = last_outcome_at
            .as_deref()
            .and_then(trimmed_non_empty)
            .map(|value| value.to_string());
        patch.last_outcome_at = Some(normalized);
    }

    Ok(patch)
}

fn default_priority() -> String {
    "medium".to_string()
}

fn default_status() -> String {
    "todo".to_string()
}

fn normalize_outcome_items(items: Vec<TaskOutcomeItem>) -> Vec<TaskOutcomeItem> {
    let mut out = Vec::new();
    for item in items {
        let kind = item.kind.trim().to_ascii_lowercase();
        let text = item.text.trim().to_string();
        if text.is_empty() {
            continue;
        }
        let importance = item
            .importance
            .as_deref()
            .and_then(trimmed_non_empty)
            .map(normalize_importance);
        let refs = normalize_string_list(item.refs);
        out.push(TaskOutcomeItem {
            kind: normalize_outcome_kind(kind.as_str()),
            text,
            importance,
            refs,
        });
    }
    out
}

fn normalize_outcome_kind(value: &str) -> String {
    match value {
        "decision" => "decision".to_string(),
        "artifact" => "artifact".to_string(),
        "risk" => "risk".to_string(),
        "handoff" => "handoff".to_string(),
        _ => "finding".to_string(),
    }
}

fn normalize_importance(value: &str) -> String {
    match value.trim().to_ascii_lowercase().as_str() {
        "high" => "high".to_string(),
        "low" => "low".to_string(),
        _ => "medium".to_string(),
    }
}

fn normalize_string_list(values: Vec<String>) -> Vec<String> {
    let mut out = Vec::new();
    for value in values {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            continue;
        }
        if out.iter().any(|item: &String| item == trimmed) {
            continue;
        }
        out.push(trimmed.to_string());
    }
    out
}

fn normalize_blocker_kind(value: &str) -> String {
    match value.trim().to_ascii_lowercase().as_str() {
        "external_dependency" => "external_dependency".to_string(),
        "permission" => "permission".to_string(),
        "missing_information" => "missing_information".to_string(),
        "design_decision" => "design_decision".to_string(),
        "environment_failure" => "environment_failure".to_string(),
        "upstream_bug" => "upstream_bug".to_string(),
        _ => "unknown".to_string(),
    }
}
