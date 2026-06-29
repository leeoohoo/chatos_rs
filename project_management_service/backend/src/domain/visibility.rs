use std::collections::HashSet;

use crate::models::{
    ProjectWorkItemRecord, ProjectWorkItemStatus, RequirementRecord, RequirementStatus,
};

pub fn should_include_archived(
    include_archived: Option<bool>,
    explicit_archived_filter: bool,
) -> bool {
    include_archived.unwrap_or(false) || explicit_archived_filter
}

pub fn non_archived_requirements(requirements: Vec<RequirementRecord>) -> Vec<RequirementRecord> {
    requirements
        .into_iter()
        .filter(|requirement| requirement.status != RequirementStatus::Archived)
        .collect()
}

pub fn non_archived_project_tasks(items: Vec<ProjectWorkItemRecord>) -> Vec<ProjectWorkItemRecord> {
    items
        .into_iter()
        .filter(|item| item.status != ProjectWorkItemStatus::Archived)
        .collect()
}

pub fn retain_project_tasks_for_requirements(
    items: Vec<ProjectWorkItemRecord>,
    requirements: &[RequirementRecord],
) -> Vec<ProjectWorkItemRecord> {
    let requirement_ids = requirements
        .iter()
        .map(|requirement| requirement.id.as_str())
        .collect::<HashSet<_>>();
    items
        .into_iter()
        .filter(|item| requirement_ids.contains(item.requirement_id.as_str()))
        .collect()
}

pub fn ensure_requirement_queryable_for_mcp(requirement: &RequirementRecord) -> Result<(), String> {
    if requirement.status == RequirementStatus::Archived {
        Err(format!("需求不存在: {}", requirement.id))
    } else {
        Ok(())
    }
}

pub fn ensure_project_task_queryable_for_mcp(item: &ProjectWorkItemRecord) -> Result<(), String> {
    if item.status == ProjectWorkItemStatus::Archived {
        Err(format!("项目任务不存在: {}", item.id))
    } else {
        Ok(())
    }
}

pub fn ensure_requirement_status_queryable_for_mcp(
    status: Option<RequirementStatus>,
) -> Result<(), String> {
    if matches!(status, Some(RequirementStatus::Archived)) {
        Err("Project Management MCP 不允许访问归档需求".to_string())
    } else {
        Ok(())
    }
}

pub fn ensure_project_task_status_queryable_for_mcp(
    status: Option<ProjectWorkItemStatus>,
) -> Result<(), String> {
    if matches!(status, Some(ProjectWorkItemStatus::Archived)) {
        Err("Project Management MCP 不允许访问归档项目任务".to_string())
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::RequirementType;

    #[test]
    fn non_archived_filters_hide_archived_records() {
        let requirements = non_archived_requirements(vec![
            requirement_record("req-active", RequirementStatus::Draft),
            requirement_record("req-archived", RequirementStatus::Archived),
        ]);
        assert_eq!(requirements.len(), 1);
        assert_eq!(requirements[0].id, "req-active");

        let items = non_archived_project_tasks(vec![
            work_item_record("item-active", ProjectWorkItemStatus::Todo),
            work_item_record("item-archived", ProjectWorkItemStatus::Archived),
        ]);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].id, "item-active");
    }

    #[test]
    fn mcp_visibility_rejects_archived_records_and_statuses() {
        assert_eq!(
            ensure_requirement_queryable_for_mcp(&requirement_record(
                "req-archived",
                RequirementStatus::Archived
            ))
            .unwrap_err(),
            "需求不存在: req-archived"
        );
        assert_eq!(
            ensure_project_task_queryable_for_mcp(&work_item_record(
                "item-archived",
                ProjectWorkItemStatus::Archived
            ))
            .unwrap_err(),
            "项目任务不存在: item-archived"
        );
        assert!(
            ensure_requirement_status_queryable_for_mcp(Some(RequirementStatus::Archived)).is_err()
        );
        assert!(ensure_project_task_status_queryable_for_mcp(Some(
            ProjectWorkItemStatus::Archived
        ))
        .is_err());
    }

    #[test]
    fn include_archived_honors_explicit_archived_filter() {
        assert!(!should_include_archived(None, false));
        assert!(should_include_archived(Some(true), false));
        assert!(should_include_archived(None, true));
    }

    fn requirement_record(id: &str, status: RequirementStatus) -> RequirementRecord {
        RequirementRecord {
            id: id.to_string(),
            project_id: "project-1".to_string(),
            parent_requirement_id: None,
            requirement_type: RequirementType::Requirement,
            title: id.to_string(),
            summary: None,
            detail: None,
            business_value: None,
            acceptance_criteria: None,
            source: None,
            priority: 0,
            status,
            creator_user_id: None,
            creator_username: None,
            creator_display_name: None,
            owner_user_id: Some("user-1".to_string()),
            owner_username: Some("owner".to_string()),
            owner_display_name: Some("Owner".to_string()),
            assignee_user_id: None,
            created_at: "now".to_string(),
            updated_at: "now".to_string(),
            archived_at: None,
        }
    }

    fn work_item_record(id: &str, status: ProjectWorkItemStatus) -> ProjectWorkItemRecord {
        ProjectWorkItemRecord {
            id: id.to_string(),
            project_id: "project-1".to_string(),
            requirement_id: "req-1".to_string(),
            title: id.to_string(),
            description: None,
            task_runner_default_model_config_id: "model-1".to_string(),
            task_runner_enabled_tool_ids: vec!["tool-1".to_string()],
            task_runner_skill_ids: Vec::new(),
            status,
            priority: 0,
            assignee_user_id: None,
            estimate_points: None,
            due_at: None,
            sort_order: 0,
            tags: Vec::new(),
            creator_user_id: None,
            creator_username: None,
            creator_display_name: None,
            owner_user_id: Some("user-1".to_string()),
            owner_username: Some("owner".to_string()),
            owner_display_name: Some("Owner".to_string()),
            created_at: "now".to_string(),
            updated_at: "now".to_string(),
            archived_at: None,
        }
    }
}
