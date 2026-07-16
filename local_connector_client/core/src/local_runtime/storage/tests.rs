// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::fs;

use serde_json::json;
use uuid::Uuid;

use super::{
    AppendLocalRuntimeEventInput, BeginLocalTurnInput, BeginLocalTurnResult,
    CompleteLocalTurnInput, CreateLocalSessionInput, LocalDatabase, UpsertLocalProjectInput,
};

mod concurrent_writes;

#[tokio::test]
async fn initializes_database_and_persists_local_project_sessions() {
    let root = std::env::temp_dir().join(format!("chatos-local-runtime-{}", Uuid::new_v4()));
    let database = LocalDatabase::open(root.join("runtime.sqlite3"))
        .await
        .expect("open local database");

    let health = database.health().await.expect("database health");
    assert!(health.ready);
    assert!(health.applied_migrations >= 1);

    let project = database
        .upsert_project(UpsertLocalProjectInput {
            project_id: "project-1".to_string(),
            owner_user_id: "user-1".to_string(),
            device_id: "device-1".to_string(),
            workspace_id: "workspace-1".to_string(),
            project_name: "Local project".to_string(),
            root_relative_path: Some("apps/backend".to_string()),
        })
        .await
        .expect("upsert project");
    assert_eq!(project.execution_plane, "local_connector");

    let session = database
        .create_session(CreateLocalSessionInput {
            project_id: project.project_id.clone(),
            owner_user_id: project.owner_user_id.clone(),
            title: "Local session".to_string(),
            selected_model_id: Some("model-1".to_string()),
            selected_agent_id: None,
        })
        .await
        .expect("create session");
    assert!(session.id.starts_with("lc_session_"));

    let sessions = database
        .list_sessions("user-1", "project-1")
        .await
        .expect("list sessions");
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].id, session.id);

    database.close().await;
    fs::remove_dir_all(root).expect("cleanup local database");
}

#[tokio::test]
async fn persists_and_reuses_idempotent_local_turns() {
    let root = std::env::temp_dir().join(format!("chatos-local-turn-{}", Uuid::new_v4()));
    let database = LocalDatabase::open(root.join("runtime.sqlite3"))
        .await
        .expect("open local database");
    database
        .upsert_project(UpsertLocalProjectInput {
            project_id: "project-1".to_string(),
            owner_user_id: "user-1".to_string(),
            device_id: "device-1".to_string(),
            workspace_id: "workspace-1".to_string(),
            project_name: "Local project".to_string(),
            root_relative_path: None,
        })
        .await
        .expect("upsert project");
    let session = database
        .create_session(CreateLocalSessionInput {
            project_id: "project-1".to_string(),
            owner_user_id: "user-1".to_string(),
            title: "Local session".to_string(),
            selected_model_id: Some("model-1".to_string()),
            selected_agent_id: None,
        })
        .await
        .expect("create session");
    let begin_input = BeginLocalTurnInput {
        session_id: session.id.clone(),
        owner_user_id: "user-1".to_string(),
        turn_id: "lc_turn_1".to_string(),
        idempotency_key: "request-1".to_string(),
        content: "hello".to_string(),
        metadata_json: Some(r#"{"conversation_turn_id":"lc_turn_1"}"#.to_string()),
    };
    let started = database
        .begin_turn(begin_input.clone())
        .await
        .expect("begin local turn");
    assert!(matches!(started, BeginLocalTurnResult::Started(_)));

    let duplicate_running = database
        .begin_turn(begin_input.clone())
        .await
        .expect("reuse running local turn");
    assert!(matches!(
        duplicate_running,
        BeginLocalTurnResult::Existing(ref snapshot) if snapshot.turn.status == "running"
    ));

    let completed = database
        .complete_turn(CompleteLocalTurnInput {
            turn_id: "lc_turn_1".to_string(),
            owner_user_id: "user-1".to_string(),
            content: "world".to_string(),
            reasoning: None,
            tool_calls_json: None,
            metadata_json: Some(r#"{"response_status":"completed"}"#.to_string()),
        })
        .await
        .expect("complete local turn");
    assert_eq!(completed.turn.status, "completed");
    assert_eq!(
        completed
            .assistant_message
            .as_ref()
            .map(|message| message.content.as_str()),
        Some("world")
    );

    let duplicate_completed = database
        .begin_turn(begin_input)
        .await
        .expect("reuse completed local turn");
    assert!(matches!(
        duplicate_completed,
        BeginLocalTurnResult::Existing(ref snapshot)
            if snapshot.turn.status == "completed" && snapshot.assistant_message.is_some()
    ));
    let messages = database
        .list_messages("user-1", session.id.as_str())
        .await
        .expect("list local messages");
    assert_eq!(messages.len(), 2);

    database.close().await;
    fs::remove_dir_all(root).expect("cleanup local database");
}

#[tokio::test]
async fn appends_and_incrementally_lists_ordered_runtime_events() {
    let root = std::env::temp_dir().join(format!("chatos-local-events-{}", Uuid::new_v4()));
    let database = LocalDatabase::open(root.join("runtime.sqlite3"))
        .await
        .expect("open local database");
    database
        .upsert_project(UpsertLocalProjectInput {
            project_id: "project-events".to_string(),
            owner_user_id: "user-events".to_string(),
            device_id: "device-events".to_string(),
            workspace_id: "workspace-events".to_string(),
            project_name: "Event project".to_string(),
            root_relative_path: None,
        })
        .await
        .expect("upsert project");
    let session = database
        .create_session(CreateLocalSessionInput {
            project_id: "project-events".to_string(),
            owner_user_id: "user-events".to_string(),
            title: "Event session".to_string(),
            selected_model_id: Some("model-events".to_string()),
            selected_agent_id: None,
        })
        .await
        .expect("create session");
    database
        .begin_turn(BeginLocalTurnInput {
            session_id: session.id.clone(),
            owner_user_id: "user-events".to_string(),
            turn_id: "lc_turn_events".to_string(),
            idempotency_key: "event-request".to_string(),
            content: "stream".to_string(),
            metadata_json: None,
        })
        .await
        .expect("begin turn");

    for (event_name, payload) in [
        ("chat.thinking", json!({ "text": "plan" })),
        ("chat.chunk", json!({ "text": "hello" })),
        ("chat.completed", json!({})),
    ] {
        database
            .append_runtime_event(AppendLocalRuntimeEventInput {
                owner_user_id: "user-events".to_string(),
                session_id: session.id.clone(),
                turn_id: "lc_turn_events".to_string(),
                event_name: event_name.to_string(),
                stream_type: Some("test".to_string()),
                payload,
            })
            .await
            .expect("append event");
    }

    let first_page = database
        .list_runtime_events(
            "user-events",
            session.id.as_str(),
            Some("lc_turn_events"),
            0,
            2,
        )
        .await
        .expect("list first event page");
    assert_eq!(first_page.len(), 2);
    assert!(first_page[0].event_seq < first_page[1].event_seq);
    assert_eq!(first_page[0].event_name, "chat.thinking");

    let remaining = database
        .list_runtime_events(
            "user-events",
            session.id.as_str(),
            Some("lc_turn_events"),
            first_page[1].event_seq,
            10,
        )
        .await
        .expect("list remaining events");
    assert_eq!(remaining.len(), 1);
    assert_eq!(remaining[0].event_name, "chat.completed");

    database.close().await;
    fs::remove_dir_all(root).expect("cleanup local database");
}
