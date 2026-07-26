// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::fs;

use uuid::Uuid;

use crate::local_runtime::project_management::{
    CreateLocalRequirementInput, CreateLocalWorkItemInput,
};
use crate::local_runtime::storage::{
    CreateLocalSessionInput, LocalDatabase, UpsertLocalProjectInput,
};
use crate::local_runtime::task_runner::EnqueueLocalTaskRunInput;

#[tokio::test]
async fn claims_each_queued_local_task_run_once() {
    let root = std::env::temp_dir().join(format!("chatos-local-task-runs-{}", Uuid::new_v4()));
    let database = LocalDatabase::open(root.join("runtime.sqlite3"))
        .await
        .expect("open database");
    let (session_id, task_id) = seed_scope(&database).await;
    let run = database
        .enqueue_local_task_run(EnqueueLocalTaskRunInput {
            owner_user_id: "user-run".to_string(),
            project_id: "project-run".to_string(),
            requirement_id: None,
            task_kind: "project_work_item".to_string(),
            task_id,
            session_id,
            execution_group_id: "group-run".to_string(),
            priority: 10,
            prompt: "Execute task".to_string(),
            model_config_id: "model-run".to_string(),
        })
        .await
        .expect("enqueue run");
    let claimed = database
        .claim_next_local_task_run("worker-1")
        .await
        .expect("claim run")
        .expect("queued run");
    assert_eq!(claimed.id, run.id);
    assert_eq!(claimed.status, "running");
    assert!(database
        .claim_next_local_task_run("worker-2")
        .await
        .expect("second claim")
        .is_none());

    database.close().await;
    fs::remove_dir_all(root).expect("cleanup database");
}

#[tokio::test]
async fn paused_local_execution_run_is_not_claimed_until_resumed() {
    let root = std::env::temp_dir().join(format!("chatos-local-task-pause-{}", Uuid::new_v4()));
    let database = LocalDatabase::open(root.join("runtime.sqlite3"))
        .await
        .expect("open database");
    let (session_id, task_id) = seed_scope(&database).await;
    let run = database
        .enqueue_local_task_run(EnqueueLocalTaskRunInput {
            owner_user_id: "user-run".to_string(),
            project_id: "project-run".to_string(),
            requirement_id: None,
            task_kind: "project_work_item".to_string(),
            task_id,
            session_id: session_id.clone(),
            execution_group_id: "group-pause".to_string(),
            priority: 10,
            prompt: "Execute task".to_string(),
            model_config_id: "model-run".to_string(),
        })
        .await
        .expect("enqueue run");
    database
        .set_local_execution_group_dispatch_paused(
            "user-run",
            "project-run",
            session_id.as_str(),
            "group-pause",
            true,
        )
        .await
        .expect("pause run");
    assert!(database
        .claim_next_local_task_run("worker-paused")
        .await
        .expect("claim paused run")
        .is_none());
    database
        .set_local_execution_group_dispatch_paused(
            "user-run",
            "project-run",
            session_id.as_str(),
            "group-pause",
            false,
        )
        .await
        .expect("resume run");
    let claimed = database
        .claim_next_local_task_run("worker-resumed")
        .await
        .expect("claim resumed run")
        .expect("resumed queued run");
    assert_eq!(claimed.id, run.id);

    database.close().await;
    fs::remove_dir_all(root).expect("cleanup database");
}

#[tokio::test]
async fn manual_retry_requeues_an_exhausted_failed_run_with_a_fresh_attempt_budget() {
    let root = std::env::temp_dir().join(format!("chatos-local-task-retry-{}", Uuid::new_v4()));
    let database = LocalDatabase::open(root.join("runtime.sqlite3"))
        .await
        .expect("open database");
    let (session_id, task_id) = seed_scope(&database).await;
    let queued = database
        .enqueue_local_task_run(EnqueueLocalTaskRunInput {
            owner_user_id: "user-run".to_string(),
            project_id: "project-run".to_string(),
            requirement_id: None,
            task_kind: "project_work_item".to_string(),
            task_id,
            session_id,
            execution_group_id: "group-retry".to_string(),
            priority: 10,
            prompt: "Retry task".to_string(),
            model_config_id: "model-original".to_string(),
        })
        .await
        .expect("enqueue run");
    sqlx::query(
        "UPDATE local_task_runs SET status = 'failed', attempt = max_attempts, error = 'boom' WHERE id = ?",
    )
    .bind(queued.id.as_str())
    .execute(database.pool())
    .await
    .expect("mark run exhausted and failed");

    let retried = database
        .retry_local_task_run(
            "user-run",
            queued.id.as_str(),
            "model-current",
            Some("配置已经修复，请重新验证"),
        )
        .await
        .expect("retry failed run")
        .expect("retryable run");

    assert_eq!(retried.status, "queued");
    assert_eq!(retried.attempt, 0);
    assert_eq!(retried.model_config_id, "model-current");
    assert_eq!(retried.error, None);
    assert!(retried.prompt.contains("配置已经修复，请重新验证"));

    database.close().await;
    fs::remove_dir_all(root).expect("cleanup database");
}

async fn seed_scope(database: &LocalDatabase) -> (String, String) {
    database
        .upsert_project(UpsertLocalProjectInput {
            project_id: "project-run".to_string(),
            owner_user_id: "user-run".to_string(),
            device_id: "device-run".to_string(),
            workspace_id: "workspace-run".to_string(),
            project_name: "Task run project".to_string(),
            root_relative_path: None,
        })
        .await
        .expect("upsert project");
    let requirement = database
        .create_local_requirement(CreateLocalRequirementInput {
            owner_user_id: "user-run".to_string(),
            project_id: "project-run".to_string(),
            parent_requirement_id: None,
            requirement_type: "requirement".to_string(),
            title: "Requirement".to_string(),
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
        .expect("create requirement");
    let task = database
        .create_local_work_item(CreateLocalWorkItemInput {
            owner_user_id: "user-run".to_string(),
            requirement_id: requirement.id,
            title: "Task".to_string(),
            description: Some("Execute task".to_string()),
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
        .expect("create work item");
    let session = database
        .create_session(CreateLocalSessionInput {
            project_id: "project-run".to_string(),
            owner_user_id: "user-run".to_string(),
            title: "Task Runner".to_string(),
            selected_model_id: Some("model-run".to_string()),
            selected_agent_id: None,
        })
        .await
        .expect("create session");
    (session.id, task.id)
}
