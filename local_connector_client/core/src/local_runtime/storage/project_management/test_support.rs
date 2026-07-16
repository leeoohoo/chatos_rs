// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::local_runtime::project_management::{
    CreateLocalRequirementInput, CreateLocalWorkItemInput, LocalRequirementRecord,
    LocalWorkItemRecord, UpsertLocalRequirementDocumentInput,
};

use super::super::LocalDatabase;

pub(super) async fn create_requirement(
    database: &LocalDatabase,
    title: &str,
) -> LocalRequirementRecord {
    database
        .create_local_requirement(CreateLocalRequirementInput {
            project_id: "project-write".to_string(),
            owner_user_id: "user-write".to_string(),
            parent_requirement_id: None,
            requirement_type: "requirement".to_string(),
            title: title.to_string(),
            summary: None,
            detail: None,
            business_value: None,
            acceptance_criteria: None,
            source: Some("test".to_string()),
            priority: 0,
            status: "approved".to_string(),
            assignee_user_id: None,
        })
        .await
        .expect("create requirement")
}

pub(super) async fn create_work_item(
    database: &LocalDatabase,
    requirement_id: &str,
    title: &str,
) -> LocalWorkItemRecord {
    database
        .create_local_work_item(CreateLocalWorkItemInput {
            requirement_id: requirement_id.to_string(),
            owner_user_id: "user-write".to_string(),
            title: title.to_string(),
            description: None,
            status: "todo".to_string(),
            priority: 0,
            assignee_user_id: None,
            estimate_points: None,
            due_at: None,
            sort_order: 0,
            tags: Vec::new(),
            is_planning_task: false,
        })
        .await
        .expect("create work item")
}

pub(super) fn document_input(
    requirement_id: &str,
    document_id: Option<String>,
    content: &str,
) -> UpsertLocalRequirementDocumentInput {
    UpsertLocalRequirementDocumentInput {
        document_id,
        requirement_id: requirement_id.to_string(),
        owner_user_id: "user-write".to_string(),
        doc_type: "implementation_plan".to_string(),
        title: "Implementation plan".to_string(),
        format: "markdown".to_string(),
        content: content.to_string(),
    }
}
