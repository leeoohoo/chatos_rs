// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::fs;

use serde_json::json;
use uuid::Uuid;

use super::super::{
    AppendLocalMessageInput, AppendLocalRuntimeEventInput, BeginLocalTurnInput,
    CreateLocalSessionInput, LocalDatabase, UpsertLocalProjectInput,
};

#[tokio::test]
async fn serializes_concurrent_message_and_event_writes() {
    let root =
        std::env::temp_dir().join(format!("chatos-local-concurrent-writes-{}", Uuid::new_v4()));
    let database = LocalDatabase::open(root.join("runtime.sqlite3"))
        .await
        .expect("open local database");
    database
        .upsert_project(UpsertLocalProjectInput {
            project_id: "project-concurrent".to_string(),
            owner_user_id: "user-concurrent".to_string(),
            device_id: "device-concurrent".to_string(),
            workspace_id: "workspace-concurrent".to_string(),
            project_name: "Concurrent writes".to_string(),
            root_relative_path: None,
        })
        .await
        .expect("upsert project");
    let session = database
        .create_session(CreateLocalSessionInput {
            project_id: "project-concurrent".to_string(),
            owner_user_id: "user-concurrent".to_string(),
            title: "Concurrent session".to_string(),
            selected_model_id: None,
            selected_agent_id: None,
        })
        .await
        .expect("create session");
    database
        .begin_turn(BeginLocalTurnInput {
            session_id: session.id.clone(),
            owner_user_id: "user-concurrent".to_string(),
            turn_id: "lc_turn_concurrent".to_string(),
            idempotency_key: "request-concurrent".to_string(),
            content: "start".to_string(),
            metadata_json: None,
        })
        .await
        .expect("begin turn");

    let mut tasks = Vec::new();
    for index in 0..16 {
        let database = database.clone();
        let session_id = session.id.clone();
        tasks.push(tokio::spawn(async move {
            database
                .append_turn_message(AppendLocalMessageInput {
                    session_id,
                    owner_user_id: "user-concurrent".to_string(),
                    turn_id: "lc_turn_concurrent".to_string(),
                    message_id: None,
                    role: "tool".to_string(),
                    content: format!("message-{index}"),
                    reasoning: None,
                    tool_calls_json: None,
                    tool_call_id: Some(format!("call-{index}")),
                    metadata_json: None,
                    created_at: None,
                })
                .await
                .map(|_| ())
        }));
    }
    for index in 0..32 {
        let database = database.clone();
        let session_id = session.id.clone();
        tasks.push(tokio::spawn(async move {
            database
                .append_runtime_event(AppendLocalRuntimeEventInput {
                    owner_user_id: "user-concurrent".to_string(),
                    session_id,
                    turn_id: "lc_turn_concurrent".to_string(),
                    event_name: "chat.tools.stream".to_string(),
                    stream_type: Some("tool".to_string()),
                    payload: json!({ "index": index }),
                })
                .await
                .map(|_| ())
        }));
    }
    for task in tasks {
        task.await
            .expect("join concurrent write")
            .expect("write data");
    }

    let messages = database
        .list_turn_messages("user-concurrent", "lc_turn_concurrent")
        .await
        .expect("list messages");
    assert_eq!(messages.len(), 17);
    assert!(messages
        .windows(2)
        .all(|pair| pair[0].sequence_no < pair[1].sequence_no));
    let events = database
        .list_runtime_events(
            "user-concurrent",
            session.id.as_str(),
            Some("lc_turn_concurrent"),
            0,
            100,
        )
        .await
        .expect("list events");
    assert_eq!(events.len(), 32);

    database.close().await;
    fs::remove_dir_all(root).expect("cleanup local database");
}
