// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::fs;

use uuid::Uuid;

use super::super::{
    BeginLocalTurnInput, BeginLocalTurnResult, CompleteLocalTurnInput,
    CreateLocalMemorySummaryInput, CreateLocalSessionInput, LocalDatabase, UpsertLocalProjectInput,
};

#[tokio::test]
async fn composes_latest_summary_with_only_newer_messages() {
    let root = std::env::temp_dir().join(format!("chatos-local-memory-{}", Uuid::new_v4()));
    let database = LocalDatabase::open(root.join("runtime.sqlite3"))
        .await
        .expect("open local database");
    database
        .upsert_project(UpsertLocalProjectInput {
            project_id: "project-memory".to_string(),
            owner_user_id: "user-memory".to_string(),
            device_id: "device-memory".to_string(),
            workspace_id: "workspace-memory".to_string(),
            project_name: "Memory project".to_string(),
            root_relative_path: None,
        })
        .await
        .expect("upsert project");
    let session = database
        .create_session(CreateLocalSessionInput {
            project_id: "project-memory".to_string(),
            owner_user_id: "user-memory".to_string(),
            title: "Memory session".to_string(),
            selected_model_id: Some("model-memory".to_string()),
            selected_agent_id: None,
        })
        .await
        .expect("create session");
    let first = database
        .begin_turn(BeginLocalTurnInput {
            session_id: session.id.clone(),
            owner_user_id: "user-memory".to_string(),
            turn_id: "lc_turn_memory_1".to_string(),
            idempotency_key: "memory-1".to_string(),
            content: "first question".to_string(),
            metadata_json: None,
        })
        .await
        .expect("begin first turn");
    let first = match first {
        BeginLocalTurnResult::Started(snapshot) => snapshot,
        _ => panic!("expected new turn"),
    };
    let completed = database
        .complete_turn(CompleteLocalTurnInput {
            turn_id: "lc_turn_memory_1".to_string(),
            owner_user_id: "user-memory".to_string(),
            content: "first answer".to_string(),
            reasoning: None,
            tool_calls_json: None,
            metadata_json: None,
        })
        .await
        .expect("complete first turn");
    let assistant_id = completed
        .assistant_message
        .as_ref()
        .expect("assistant message")
        .id
        .clone();
    database
        .create_memory_summary(CreateLocalMemorySummaryInput {
            owner_user_id: "user-memory".to_string(),
            session_id: session.id.clone(),
            summary_text: "The first exchange is remembered.".to_string(),
            summary_model: "test-model".to_string(),
            trigger_type: "manual_review_repair".to_string(),
            source_start_message_id: Some(first.user_message.id),
            source_end_message_id: Some(assistant_id),
            source_message_count: 2,
            source_estimated_tokens: 12,
            level: 0,
        })
        .await
        .expect("create summary");
    database
        .begin_turn(BeginLocalTurnInput {
            session_id: session.id.clone(),
            owner_user_id: "user-memory".to_string(),
            turn_id: "lc_turn_memory_2".to_string(),
            idempotency_key: "memory-2".to_string(),
            content: "second question".to_string(),
            metadata_json: None,
        })
        .await
        .expect("begin second turn");

    let context = database
        .load_memory_context("user-memory", session.id.as_str(), 8)
        .await
        .expect("load memory context");
    assert_eq!(
        context
            .summary
            .as_ref()
            .map(|summary| summary.summary_text.as_str()),
        Some("The first exchange is remembered.")
    );
    assert_eq!(context.messages.len(), 1);
    assert_eq!(context.messages[0].content, "second question");
    assert_eq!(
        database
            .count_pending_memory_messages("user-memory", session.id.as_str())
            .await
            .expect("count pending messages"),
        0
    );
    database
        .complete_turn(CompleteLocalTurnInput {
            turn_id: "lc_turn_memory_2".to_string(),
            owner_user_id: "user-memory".to_string(),
            content: "second answer".to_string(),
            reasoning: None,
            tool_calls_json: None,
            metadata_json: None,
        })
        .await
        .expect("complete second turn");
    assert_eq!(
        database
            .count_pending_memory_messages("user-memory", session.id.as_str())
            .await
            .expect("count completed pending messages"),
        2
    );

    database.close().await;
    fs::remove_dir_all(root).expect("cleanup local memory database");
}
