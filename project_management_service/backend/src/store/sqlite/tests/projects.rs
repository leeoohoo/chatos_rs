// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::support::{test_agent_user, test_store};
use crate::models::*;

#[tokio::test]
async fn agent_created_records_keep_agent_creator_and_real_owner() {
    let store = test_store().await;
    let agent = test_agent_user();
    let project = store
        .create_project(
            CreateProjectRequest {
                name: "Agent Project".to_string(),
                root_path: None,
                git_url: None,
                description: None,
            },
            &agent,
        )
        .await
        .expect("create project");
    let profile = store
        .upsert_project_profile(
            &project.id,
            UpsertProjectProfileRequest {
                background: Some("Background".to_string()),
                introduction: Some("Intro".to_string()),
            },
            &agent,
        )
        .await
        .expect("upsert profile");
    let requirement = store
        .create_requirement(
            &project.id,
            CreateRequirementRequest {
                parent_requirement_id: None,
                requirement_type: None,
                title: "Requirement".to_string(),
                summary: None,
                detail: None,
                business_value: None,
                acceptance_criteria: None,
                source: None,
                priority: None,
                status: None,
                assignee_user_id: None,
            },
            &agent,
        )
        .await
        .expect("create requirement");
    let document = store
        .upsert_requirement_document(
            &requirement.id,
            UpsertRequirementDocumentRequest {
                doc_type: None,
                title: None,
                format: None,
                content: "Technical overview".to_string(),
            },
            &agent,
        )
        .await
        .expect("upsert document");
    let item = store
        .create_work_item(
            &requirement,
            CreateProjectWorkItemRequest {
                title: "Work item".to_string(),
                description: None,
                task_runner_default_model_config_id: "model-config-test".to_string(),
                task_runner_enabled_tool_ids: vec!["filesystem".to_string()],
                task_runner_skill_ids: Vec::new(),
                status: None,
                priority: None,
                assignee_user_id: None,
                estimate_points: None,
                due_at: None,
                sort_order: None,
                tags: None,
                is_planning_task: false,
            },
            &agent,
        )
        .await
        .expect("create work item");

    for (creator_user_id, owner_user_id) in [
        (
            project.creator_user_id.as_deref(),
            project.owner_user_id.as_deref(),
        ),
        (
            profile.creator_user_id.as_deref(),
            profile.owner_user_id.as_deref(),
        ),
        (
            requirement.creator_user_id.as_deref(),
            requirement.owner_user_id.as_deref(),
        ),
        (
            document.creator_user_id.as_deref(),
            document.owner_user_id.as_deref(),
        ),
        (
            item.creator_user_id.as_deref(),
            item.owner_user_id.as_deref(),
        ),
    ] {
        assert_eq!(creator_user_id, Some("agent-1"));
        assert_eq!(owner_user_id, Some("user-1"));
    }
}
