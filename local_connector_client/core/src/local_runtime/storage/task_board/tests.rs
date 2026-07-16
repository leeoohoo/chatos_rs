// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::fs;

use chatos_builtin_tools::{TaskDraft, TaskOutcomeItem, TaskUpdatePatch};
use uuid::Uuid;

use super::super::{
    BeginLocalTurnInput, BeginLocalTurnResult, CreateLocalSessionInput, LocalDatabase,
    UpsertLocalProjectInput,
};

#[tokio::test]
async fn persists_and_mutates_local_task_board() {
    let root = std::env::temp_dir().join(format!("chatos-local-task-board-{}", Uuid::new_v4()));
    let database = LocalDatabase::open(root.join("runtime.sqlite3"))
        .await
        .expect("open local task board database");
    database
        .upsert_project(UpsertLocalProjectInput {
            project_id: "task-project".to_string(),
            owner_user_id: "task-user".to_string(),
            device_id: "task-device".to_string(),
            workspace_id: "task-workspace".to_string(),
            project_name: "Task project".to_string(),
            root_relative_path: None,
        })
        .await
        .expect("upsert task project");
    let session = database
        .create_session(CreateLocalSessionInput {
            project_id: "task-project".to_string(),
            owner_user_id: "task-user".to_string(),
            title: "Task session".to_string(),
            selected_model_id: None,
            selected_agent_id: None,
        })
        .await
        .expect("create task session");
    let turn = database
        .begin_turn(BeginLocalTurnInput {
            session_id: session.id.clone(),
            owner_user_id: "task-user".to_string(),
            turn_id: "task-turn".to_string(),
            idempotency_key: "task-turn".to_string(),
            content: "Create tasks".to_string(),
            metadata_json: None,
        })
        .await
        .expect("begin task turn");
    let turn_id = match turn {
        BeginLocalTurnResult::Started(snapshot) => snapshot.turn.id,
        BeginLocalTurnResult::Existing(_) => panic!("unexpected existing turn"),
    };

    let first = database
        .create_local_task_board_tasks(
            "task-user",
            session.id.as_str(),
            turn_id.as_str(),
            vec![draft("Inspect local runtime")],
        )
        .await
        .expect("create first task")
        .remove(0);
    let mut dependent = draft("Implement task board");
    dependent.prerequisite_task_ids = vec![first.id.clone()];
    let second = database
        .create_local_task_board_tasks(
            "task-user",
            session.id.as_str(),
            turn_id.as_str(),
            vec![dependent],
        )
        .await
        .expect("create dependent task")
        .remove(0);
    assert_eq!(second.prerequisite_task_ids, vec![first.id.clone()]);

    assert!(database
        .complete_local_task_board_task(
            "task-user",
            session.id.as_str(),
            second.id.as_str(),
            TaskUpdatePatch::default(),
        )
        .await
        .is_err());
    let completed = database
        .complete_local_task_board_task(
            "task-user",
            session.id.as_str(),
            first.id.as_str(),
            TaskUpdatePatch {
                outcome_summary: Some("Runtime inspected".to_string()),
                outcome_items: Some(vec![TaskOutcomeItem {
                    kind: "finding".to_string(),
                    text: "SQLite is ready".to_string(),
                    importance: Some("high".to_string()),
                    refs: Vec::new(),
                }]),
                ..Default::default()
            },
        )
        .await
        .expect("complete first task");
    assert_eq!(completed.status, "done");
    assert!(completed.completed_at.is_some());

    let prompt = database
        .local_task_board_prompt("task-user", session.id.as_str())
        .await
        .expect("format task board prompt");
    assert!(prompt.contains("Runtime inspected"));
    assert!(prompt.contains(second.id.as_str()));

    assert!(database
        .delete_local_task_board_task("task-user", session.id.as_str(), second.id.as_str())
        .await
        .expect("delete dependent task"));
    assert_eq!(
        database
            .list_local_task_board_tasks("task-user", session.id.as_str(), None, true, 20)
            .await
            .expect("list remaining tasks")
            .len(),
        1
    );

    database.close().await;
    fs::remove_dir_all(root).expect("cleanup local task board database");
}

fn draft(title: &str) -> TaskDraft {
    TaskDraft {
        title: title.to_string(),
        details: String::new(),
        priority: "medium".to_string(),
        status: "todo".to_string(),
        tags: vec!["local".to_string()],
        prerequisite_task_id: None,
        prerequisite_task_ids: Vec::new(),
        due_at: None,
        outcome_summary: String::new(),
        outcome_items: Vec::new(),
        resume_hint: String::new(),
        blocker_reason: String::new(),
        blocker_needs: Vec::new(),
        blocker_kind: String::new(),
    }
}
