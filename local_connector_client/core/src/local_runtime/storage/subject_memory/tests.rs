// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::fs;

use uuid::Uuid;

use super::super::{
    BeginLocalTurnInput, BeginLocalTurnResult, CompleteLocalTurnInput,
    CreateLocalMemorySummaryInput, CreateLocalSessionInput, LocalDatabase, UpsertLocalProjectInput,
};

#[tokio::test]
async fn recalls_project_and_agent_memory_only_from_other_sessions() {
    let root = std::env::temp_dir().join(format!("chatos-subject-memory-{}", Uuid::new_v4()));
    let database = LocalDatabase::open(root.join("runtime.sqlite3"))
        .await
        .expect("open local database");
    database
        .upsert_project(UpsertLocalProjectInput {
            project_id: "project-recall".to_string(),
            owner_user_id: "user-recall".to_string(),
            device_id: "device-recall".to_string(),
            workspace_id: "workspace-recall".to_string(),
            project_name: "Recall project".to_string(),
            root_relative_path: None,
        })
        .await
        .expect("upsert project");
    let first_session = database
        .create_session(CreateLocalSessionInput {
            project_id: "project-recall".to_string(),
            owner_user_id: "user-recall".to_string(),
            title: "First recall session".to_string(),
            selected_model_id: Some("model-recall".to_string()),
            selected_agent_id: Some("agent-recall".to_string()),
        })
        .await
        .expect("create first session");
    let second_session = database
        .create_session(CreateLocalSessionInput {
            project_id: "project-recall".to_string(),
            owner_user_id: "user-recall".to_string(),
            title: "Second recall session".to_string(),
            selected_model_id: Some("model-recall".to_string()),
            selected_agent_id: Some("agent-recall".to_string()),
        })
        .await
        .expect("create second session");
    let started = database
        .begin_turn(BeginLocalTurnInput {
            session_id: first_session.id.clone(),
            owner_user_id: "user-recall".to_string(),
            turn_id: "lc_turn_recall".to_string(),
            idempotency_key: "recall-turn".to_string(),
            content: "remember the architecture".to_string(),
            metadata_json: None,
        })
        .await
        .expect("begin turn");
    let started = match started {
        BeginLocalTurnResult::Started(snapshot) => snapshot,
        _ => panic!("expected started turn"),
    };
    let completed = database
        .complete_turn(CompleteLocalTurnInput {
            turn_id: "lc_turn_recall".to_string(),
            owner_user_id: "user-recall".to_string(),
            content: "architecture remembered".to_string(),
            reasoning: None,
            tool_calls_json: None,
            metadata_json: None,
        })
        .await
        .expect("complete turn");
    let summary = database
        .create_memory_summary(CreateLocalMemorySummaryInput {
            owner_user_id: "user-recall".to_string(),
            session_id: first_session.id.clone(),
            summary_text: "The project uses local SQLite orchestration.".to_string(),
            summary_model: "test-model".to_string(),
            trigger_type: "test".to_string(),
            source_start_message_id: Some(started.user_message.id),
            source_end_message_id: completed.assistant_message.map(|message| message.id),
            source_message_count: 2,
            source_estimated_tokens: 10,
            level: 0,
        })
        .await
        .expect("create summary");
    database
        .upsert_subject_memories_for_summary("user-recall", &first_session, &summary)
        .await
        .expect("upsert subject memories");

    let recalled = database
        .list_subject_memories_for_session("user-recall", second_session.id.as_str(), 10)
        .await
        .expect("list second session recalls");
    assert_eq!(recalled.len(), 1);
    assert_eq!(recalled[0].subject_type, "agent");
    assert!(recalled
        .iter()
        .all(|item| item.source_session_id == first_session.id));
    let recall_key = format!("session:{}", first_session.id);
    assert!(database
        .get_subject_memory(
            "user-recall",
            "project",
            "project-recall",
            "project-recall",
            recall_key.as_str(),
        )
        .await
        .expect("load project recall")
        .is_some());
    assert!(database
        .list_subject_memories_for_session("user-recall", first_session.id.as_str(), 10)
        .await
        .expect("list own session recalls")
        .is_empty());

    let deleted = database
        .forget_subject_memory_for_session(
            "user-recall",
            second_session.id.as_str(),
            recalled[0].id.as_str(),
        )
        .await
        .expect("forget recalled source");
    assert_eq!(deleted, 2);
    assert!(database
        .list_subject_memories_for_session("user-recall", second_session.id.as_str(), 10)
        .await
        .expect("list forgotten recalls")
        .is_empty());

    let replacement_summary = database
        .create_memory_summary(CreateLocalMemorySummaryInput {
            owner_user_id: "user-recall".to_string(),
            session_id: first_session.id.clone(),
            summary_text: "A newer cumulative summary still contains the decision.".to_string(),
            summary_model: "test-model".to_string(),
            trigger_type: "test".to_string(),
            source_start_message_id: None,
            source_end_message_id: None,
            source_message_count: 3,
            source_estimated_tokens: 12,
            level: 0,
        })
        .await
        .expect("create replacement summary");
    let regenerated = database
        .upsert_subject_memories_for_summary("user-recall", &first_session, &replacement_summary)
        .await
        .expect("skip forgotten subject memories");
    assert!(regenerated.is_empty());
    let marker_count = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM subject_memory_forget_markers WHERE owner_user_id = ?",
    )
    .bind("user-recall")
    .fetch_one(database.pool())
    .await
    .expect("count forget markers");
    assert_eq!(marker_count, 2);

    database.close().await;
    fs::remove_dir_all(root).expect("cleanup subject memory database");
}
