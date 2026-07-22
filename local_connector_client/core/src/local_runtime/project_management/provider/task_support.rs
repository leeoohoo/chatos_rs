// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_mcp::project_management_contract::args::ProjectTaskStatus;

use crate::local_runtime::project_management::{is_completed_project_status, LocalWorkItemRecord};

use super::requirement_support::require_mutable as require_requirement_mutable;
use super::LocalProjectManagementProvider;

pub(super) async fn require_mutable(
    provider: &LocalProjectManagementProvider,
    work_item_id: &str,
) -> Result<LocalWorkItemRecord, String> {
    let record = provider
        .database
        .get_local_work_item(provider.owner_user_id.as_str(), work_item_id)
        .await
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "local project task was not found".to_string())?;
    if record.project_id != provider.project_id || record.archived_at.is_some() {
        return Err("local project task was not found".to_string());
    }
    if is_completed_project_status(record.status.as_str()) {
        return Err("completed local project task is immutable".to_string());
    }
    require_requirement_mutable(provider, record.requirement_id.as_str()).await?;
    Ok(record)
}

pub(super) fn matches_keyword(record: &LocalWorkItemRecord, keyword: &str) -> bool {
    [
        Some(record.id.as_str()),
        Some(record.requirement_id.as_str()),
        Some(record.title.as_str()),
        record.description.as_deref(),
    ]
    .into_iter()
    .flatten()
    .chain(record.tags.iter().map(String::as_str))
    .any(|value| value.to_lowercase().contains(keyword))
}

pub(super) fn status(value: ProjectTaskStatus) -> String {
    match value {
        ProjectTaskStatus::Todo => "todo",
        ProjectTaskStatus::Ready => "ready",
        ProjectTaskStatus::InProgress => "in_progress",
        ProjectTaskStatus::Blocked => "blocked",
        ProjectTaskStatus::Failed => "failed",
        ProjectTaskStatus::Done => "done",
        ProjectTaskStatus::Cancelled => "cancelled",
        ProjectTaskStatus::Archived => "archived",
    }
    .to_string()
}

pub(super) fn required_title(value: String) -> Result<String, String> {
    let value = value.trim().to_string();
    (!value.is_empty())
        .then_some(value)
        .ok_or_else(|| "title is required".to_string())
}
