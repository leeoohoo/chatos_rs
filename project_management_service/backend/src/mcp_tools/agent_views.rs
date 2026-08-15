// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde_json::{json, Value};

use crate::models::{
    ProjectProfileRecord, ProjectRecord, ProjectWorkItemRecord, RequirementDependencyRecord,
    RequirementDocumentRecord, RequirementRecord, WorkItemDependencyRecord,
};

pub(super) fn project_overview(project: &ProjectRecord, profile: &ProjectProfileRecord) -> Value {
    json!({
        "project": {
            "name": project.name,
            "description": project.description,
            "status": project.status,
        },
        "profile": {
            "background": profile.background,
            "introduction": profile.introduction,
        }
    })
}

pub(super) fn requirement(record: &RequirementRecord) -> Value {
    json!({
        "id": record.id,
        "parent_requirement_id": record.parent_requirement_id,
        "requirement_type": record.requirement_type,
        "title": record.title,
        "summary": record.summary,
        "detail": record.detail,
        "business_value": record.business_value,
        "acceptance_criteria": record.acceptance_criteria,
        "source": record.source,
        "priority": record.priority,
        "status": record.status,
    })
}

pub(super) fn requirement_document_summary(record: &RequirementDocumentRecord) -> Value {
    json!({
        "id": record.id,
        "requirement_id": record.requirement_id,
        "doc_type": record.doc_type,
        "title": record.title,
        "format": record.format,
        "version": record.version,
    })
}

pub(super) fn requirement_document(record: &RequirementDocumentRecord) -> Value {
    let mut view = requirement_document_summary(record);
    view.as_object_mut()
        .expect("requirement document view is an object")
        .insert("content".to_string(), json!(record.content));
    view
}

pub(super) fn project_task(record: &ProjectWorkItemRecord) -> Value {
    json!({
        "id": record.id,
        "requirement_id": record.requirement_id,
        "title": record.title,
        "description": record.description,
        "status": record.status,
        "priority": record.priority,
        "estimate_points": record.estimate_points,
        "due_at": record.due_at,
        "sort_order": record.sort_order,
        "tags": record.tags,
        "owned_paths": record.owned_paths,
        "is_planning_task": record.is_planning_task,
    })
}

pub(super) fn requirement_dependency(record: &RequirementDependencyRecord) -> Value {
    json!({
        "requirement_id": record.requirement_id,
        "prerequisite_requirement_id": record.prerequisite_requirement_id,
        "relation_type": record.relation_type,
    })
}

pub(super) fn project_task_dependency(record: &WorkItemDependencyRecord) -> Value {
    json!({
        "project_task_id": record.work_item_id,
        "prerequisite_project_task_id": record.prerequisite_work_item_id,
        "relation_type": record.relation_type,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{
        CloudImportSource, ProjectExecutionPlane, ProjectImportStatus, ProjectSourceType,
        ProjectStatus, ProjectWorkItemStatus, RequirementStatus, RequirementType,
    };

    fn project_record() -> ProjectRecord {
        ProjectRecord {
            id: "project-1".to_string(),
            creator_user_id: Some("creator-1".to_string()),
            creator_username: Some("creator@example.com".to_string()),
            creator_display_name: Some("Creator".to_string()),
            owner_user_id: Some("owner-1".to_string()),
            owner_username: Some("owner@example.com".to_string()),
            owner_display_name: Some("Owner".to_string()),
            name: "Project".to_string(),
            root_path: Some("/private/workspace".to_string()),
            git_url: Some("ssh://private/repository".to_string()),
            source_type: ProjectSourceType::Cloud,
            execution_plane: ProjectExecutionPlane::Cloud,
            cloud_import_source: CloudImportSource::Git,
            import_status: ProjectImportStatus::Ready,
            source_git_url: Some("ssh://source/repository".to_string()),
            harness_space_identifier: Some("space-1".to_string()),
            harness_repo_identifier: Some("repo-1".to_string()),
            harness_repo_path: Some("owner/repo".to_string()),
            harness_git_url: Some("http://harness/private.git".to_string()),
            harness_git_ssh_url: Some("ssh://harness/private.git".to_string()),
            harness_default_branch: Some("main".to_string()),
            harness_provision_status: Some("ready".to_string()),
            harness_provision_error: None,
            harness_provisioned_at: Some("now".to_string()),
            import_error: None,
            import_started_at: Some("now".to_string()),
            import_finished_at: Some("now".to_string()),
            description: Some("Description".to_string()),
            status: ProjectStatus::Active,
            created_at: "now".to_string(),
            updated_at: "now".to_string(),
            archived_at: None,
        }
    }

    fn profile_record() -> ProjectProfileRecord {
        ProjectProfileRecord {
            project_id: "project-1".to_string(),
            creator_user_id: Some("creator-1".to_string()),
            creator_username: Some("creator@example.com".to_string()),
            creator_display_name: Some("Creator".to_string()),
            owner_user_id: Some("owner-1".to_string()),
            owner_username: Some("owner@example.com".to_string()),
            owner_display_name: Some("Owner".to_string()),
            background: Some("Background".to_string()),
            introduction: Some("Introduction".to_string()),
            created_at: "now".to_string(),
            updated_at: "now".to_string(),
        }
    }

    fn requirement_record() -> RequirementRecord {
        RequirementRecord {
            id: "requirement-1".to_string(),
            project_id: "project-1".to_string(),
            parent_requirement_id: None,
            requirement_type: RequirementType::Requirement,
            title: "Requirement".to_string(),
            summary: Some("Summary".to_string()),
            detail: Some("Detail".to_string()),
            business_value: Some("Value".to_string()),
            acceptance_criteria: Some("Criteria".to_string()),
            source: Some("user".to_string()),
            priority: 10,
            status: RequirementStatus::Approved,
            creator_user_id: Some("creator-1".to_string()),
            creator_username: Some("creator@example.com".to_string()),
            creator_display_name: Some("Creator".to_string()),
            owner_user_id: Some("owner-1".to_string()),
            owner_username: Some("owner@example.com".to_string()),
            owner_display_name: Some("Owner".to_string()),
            assignee_user_id: Some("assignee-1".to_string()),
            created_at: "now".to_string(),
            updated_at: "now".to_string(),
            archived_at: None,
        }
    }

    fn project_task_record() -> ProjectWorkItemRecord {
        ProjectWorkItemRecord {
            id: "task-1".to_string(),
            project_id: "project-1".to_string(),
            requirement_id: "requirement-1".to_string(),
            title: "Task".to_string(),
            description: Some("Description".to_string()),
            status: ProjectWorkItemStatus::Ready,
            priority: 5,
            assignee_user_id: Some("assignee-1".to_string()),
            estimate_points: Some(3),
            due_at: Some("later".to_string()),
            sort_order: 1,
            tags: vec!["frontend".to_string()],
            owned_paths: vec!["src/components".to_string()],
            is_planning_task: false,
            creator_user_id: Some("creator-1".to_string()),
            creator_username: Some("creator@example.com".to_string()),
            creator_display_name: Some("Creator".to_string()),
            owner_user_id: Some("owner-1".to_string()),
            owner_username: Some("owner@example.com".to_string()),
            owner_display_name: Some("Owner".to_string()),
            created_at: "now".to_string(),
            updated_at: "now".to_string(),
            archived_at: None,
        }
    }

    fn assert_internal_fields_absent(value: &Value) {
        for field in [
            "project_id",
            "creator_user_id",
            "creator_username",
            "creator_display_name",
            "owner_user_id",
            "owner_username",
            "owner_display_name",
            "assignee_user_id",
            "root_path",
            "git_url",
            "source_type",
            "execution_plane",
            "cloud_import_source",
            "import_status",
            "source_git_url",
            "harness_space_identifier",
            "harness_repo_identifier",
            "harness_repo_path",
            "harness_git_url",
            "harness_git_ssh_url",
            "harness_default_branch",
            "harness_provision_status",
            "harness_provision_error",
            "harness_provisioned_at",
            "created_at",
            "updated_at",
            "archived_at",
        ] {
            assert!(
                value.get(field).is_none(),
                "unexpected field {field}: {value}"
            );
        }
    }

    #[test]
    fn project_overview_only_exposes_business_context() {
        let view = project_overview(&project_record(), &profile_record());
        assert_eq!(view.pointer("/project/name"), Some(&json!("Project")));
        assert_eq!(
            view.pointer("/profile/background"),
            Some(&json!("Background"))
        );
        assert_internal_fields_absent(view.get("project").expect("project"));
        assert_internal_fields_absent(view.get("profile").expect("profile"));
    }

    #[test]
    fn requirement_and_task_views_keep_tool_chain_ids_without_identity_metadata() {
        let requirement_view = requirement(&requirement_record());
        assert_eq!(requirement_view.get("id"), Some(&json!("requirement-1")));
        assert_internal_fields_absent(&requirement_view);

        let task_view = project_task(&project_task_record());
        assert_eq!(task_view.get("id"), Some(&json!("task-1")));
        assert_eq!(
            task_view.get("requirement_id"),
            Some(&json!("requirement-1"))
        );
        assert_internal_fields_absent(&task_view);
    }

    #[test]
    fn document_list_view_omits_content_and_identity_metadata() {
        let document = RequirementDocumentRecord {
            id: "document-1".to_string(),
            requirement_id: "requirement-1".to_string(),
            doc_type: "technical_overview".to_string(),
            creator_user_id: Some("creator-1".to_string()),
            creator_username: Some("creator@example.com".to_string()),
            creator_display_name: Some("Creator".to_string()),
            owner_user_id: Some("owner-1".to_string()),
            owner_username: Some("owner@example.com".to_string()),
            owner_display_name: Some("Owner".to_string()),
            title: "Document".to_string(),
            format: "markdown".to_string(),
            content: "Long content".to_string(),
            version: 2,
            created_at: "now".to_string(),
            updated_at: "now".to_string(),
        };
        let summary = requirement_document_summary(&document);
        assert!(summary.get("content").is_none());
        assert_internal_fields_absent(&summary);

        let detail = requirement_document(&document);
        assert_eq!(detail.get("content"), Some(&json!("Long content")));
        assert_internal_fields_absent(&detail);
    }
}
