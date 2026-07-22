// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_mcp::project_management_contract::args::{RequirementStatus, RequirementType};

use crate::local_runtime::project_management::{
    is_completed_project_status, LocalRequirementRecord,
};

use super::LocalProjectManagementProvider;

pub(super) async fn require_mutable(
    provider: &LocalProjectManagementProvider,
    requirement_id: &str,
) -> Result<LocalRequirementRecord, String> {
    let record = provider
        .database
        .get_local_requirement(provider.owner_user_id.as_str(), requirement_id)
        .await
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "local requirement was not found".to_string())?;
    if record.project_id != provider.project_id || record.archived_at.is_some() {
        return Err("local requirement was not found".to_string());
    }
    if is_completed_project_status(record.status.as_str()) {
        return Err("completed local requirement is immutable".to_string());
    }
    Ok(record)
}

pub(super) fn required_text(value: String, field: &str) -> Result<String, String> {
    let value = value.trim().to_string();
    (!value.is_empty())
        .then_some(value)
        .ok_or_else(|| format!("{field} is required"))
}

pub(super) fn matches_keyword(record: &LocalRequirementRecord, keyword: &str) -> bool {
    [
        Some(record.id.as_str()),
        Some(record.title.as_str()),
        record.summary.as_deref(),
        record.detail.as_deref(),
        record.business_value.as_deref(),
        record.acceptance_criteria.as_deref(),
        record.source.as_deref(),
    ]
    .into_iter()
    .flatten()
    .any(|value| value.to_lowercase().contains(keyword))
}

pub(super) fn requirement_status(value: RequirementStatus) -> String {
    match value {
        RequirementStatus::Draft => "draft",
        RequirementStatus::Reviewing => "reviewing",
        RequirementStatus::Approved => "approved",
        RequirementStatus::InProgress => "in_progress",
        RequirementStatus::Blocked => "blocked",
        RequirementStatus::Failed => "failed",
        RequirementStatus::Done => "done",
        RequirementStatus::Cancelled => "cancelled",
        RequirementStatus::Archived => "archived",
    }
    .to_string()
}

pub(super) fn requirement_type(value: RequirementType) -> String {
    match value {
        RequirementType::Requirement => "requirement",
        RequirementType::Change => "change",
        RequirementType::BugFix => "bug_fix",
    }
    .to_string()
}
