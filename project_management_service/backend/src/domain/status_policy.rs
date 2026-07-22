// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::models::{ProjectWorkItemStatus, RequirementStatus};

pub fn ensure_requirement_create_status(status: Option<RequirementStatus>) -> Result<(), String> {
    if matches!(status, None | Some(RequirementStatus::Draft)) {
        return Ok(());
    }
    Err(
        "创建需求时状态只能省略或为 draft；reviewing/approved 请在规划完成后更新，执行状态由系统维护"
            .to_string(),
    )
}

pub fn ensure_requirement_user_update_status(
    status: Option<RequirementStatus>,
) -> Result<(), String> {
    if matches!(
        status,
        None | Some(RequirementStatus::Draft)
            | Some(RequirementStatus::Reviewing)
            | Some(RequirementStatus::Approved)
    ) {
        return Ok(());
    }
    Err(
        "需求的 in_progress/blocked/failed/done/cancelled/archived 状态由执行系统维护；普通更新只能设置 draft、reviewing 或 approved"
            .to_string(),
    )
}

pub fn ensure_project_task_create_status(
    status: Option<ProjectWorkItemStatus>,
) -> Result<(), String> {
    if matches!(status, None | Some(ProjectWorkItemStatus::Todo)) {
        return Ok(());
    }
    Err("创建项目任务时状态只能省略或为 todo；执行状态由系统维护".to_string())
}

pub fn ensure_project_task_user_update_status(
    status: Option<ProjectWorkItemStatus>,
) -> Result<(), String> {
    if matches!(
        status,
        None | Some(ProjectWorkItemStatus::Todo) | Some(ProjectWorkItemStatus::Ready)
    ) {
        return Ok(());
    }
    Err(
        "项目任务的 in_progress/blocked/failed/done/cancelled/archived 状态由执行系统维护；普通更新只能设置 todo 或 ready"
            .to_string(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requirement_creation_keeps_draft_as_the_only_explicit_status() {
        assert!(ensure_requirement_create_status(None).is_ok());
        assert!(ensure_requirement_create_status(Some(RequirementStatus::Draft)).is_ok());
        assert!(ensure_requirement_create_status(Some(RequirementStatus::Approved)).is_err());
        assert!(ensure_requirement_create_status(Some(RequirementStatus::InProgress)).is_err());
    }

    #[test]
    fn requirement_updates_reject_execution_managed_statuses() {
        for status in [
            RequirementStatus::Draft,
            RequirementStatus::Reviewing,
            RequirementStatus::Approved,
        ] {
            assert!(ensure_requirement_user_update_status(Some(status)).is_ok());
        }
        for status in [
            RequirementStatus::InProgress,
            RequirementStatus::Blocked,
            RequirementStatus::Failed,
            RequirementStatus::Done,
            RequirementStatus::Cancelled,
            RequirementStatus::Archived,
        ] {
            assert!(ensure_requirement_user_update_status(Some(status)).is_err());
        }
    }

    #[test]
    fn project_task_creation_and_updates_reject_execution_managed_statuses() {
        assert!(ensure_project_task_create_status(None).is_ok());
        assert!(ensure_project_task_create_status(Some(ProjectWorkItemStatus::Todo)).is_ok());
        assert!(ensure_project_task_create_status(Some(ProjectWorkItemStatus::Ready)).is_err());
        assert!(ensure_project_task_user_update_status(Some(ProjectWorkItemStatus::Todo)).is_ok());
        assert!(ensure_project_task_user_update_status(Some(ProjectWorkItemStatus::Ready)).is_ok());
        assert!(
            ensure_project_task_user_update_status(Some(ProjectWorkItemStatus::InProgress))
                .is_err()
        );
        assert!(ensure_project_task_user_update_status(Some(ProjectWorkItemStatus::Done)).is_err());
    }
}
