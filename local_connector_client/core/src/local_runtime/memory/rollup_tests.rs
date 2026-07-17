// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::fs;
use std::sync::atomic::Ordering;

use uuid::Uuid;

use crate::local_runtime::storage::{
    BeginLocalTurnInput, CompleteLocalTurnInput, CreateLocalSessionInput, LocalDatabase,
    UpsertLocalProjectInput,
};

use super::rollup_test_support::{seed_recall, start_provider, test_runtime, OWNER, PROJECT};
use super::service::run_review_inner;

#[tokio::test]
async fn generates_recall_rollup_with_the_local_model() {
    let (provider_url, provider_calls, provider_task) = start_provider().await;
    let root = std::env::temp_dir().join(format!("chatos-memory-rollup-{}", Uuid::new_v4()));
    let database = LocalDatabase::open(root.join("runtime.sqlite3"))
        .await
        .expect("open local database");
    database
        .upsert_project(UpsertLocalProjectInput {
            project_id: PROJECT.to_string(),
            owner_user_id: OWNER.to_string(),
            device_id: "device-memory-rollup".to_string(),
            workspace_id: "workspace-memory-rollup".to_string(),
            project_name: "Memory rollup project".to_string(),
            root_relative_path: None,
        })
        .await
        .expect("upsert project");
    for index in 0..2 {
        seed_recall(&database, index).await;
    }
    let session = database
        .create_session(CreateLocalSessionInput {
            project_id: PROJECT.to_string(),
            owner_user_id: OWNER.to_string(),
            title: "Current memory session".to_string(),
            selected_model_id: Some("model-memory-rollup".to_string()),
            selected_agent_id: None,
        })
        .await
        .expect("create current session");
    sqlx::query("UPDATE session_runtime_settings SET memory_recall_limit = 2 WHERE session_id = ?")
        .bind(session.id.as_str())
        .execute(database.pool())
        .await
        .expect("set recall limit");
    database
        .begin_turn(BeginLocalTurnInput {
            session_id: session.id.clone(),
            owner_user_id: OWNER.to_string(),
            turn_id: "lc_turn_memory_rollup".to_string(),
            idempotency_key: "memory-rollup".to_string(),
            content: "Preserve this local decision.".to_string(),
            metadata_json: None,
        })
        .await
        .expect("begin memory turn");
    database
        .complete_turn(CompleteLocalTurnInput {
            turn_id: "lc_turn_memory_rollup".to_string(),
            owner_user_id: OWNER.to_string(),
            content: "Decision preserved.".to_string(),
            reasoning: None,
            tool_calls_json: None,
            metadata_json: None,
        })
        .await
        .expect("complete memory turn");
    let runtime = test_runtime(&root, provider_url, database.clone());

    let result = run_review_inner(&runtime, OWNER, session.id.as_str(), "test")
        .await
        .expect("run local memory review");
    assert_eq!(result.generated_summaries, 1);
    assert_eq!(provider_calls.load(Ordering::SeqCst), 2);

    let target = database
        .create_session(CreateLocalSessionInput {
            project_id: PROJECT.to_string(),
            owner_user_id: OWNER.to_string(),
            title: "Recall target".to_string(),
            selected_model_id: Some("model-memory-rollup".to_string()),
            selected_agent_id: None,
        })
        .await
        .expect("create recall target");
    let recalls = database
        .list_subject_memories_for_session(OWNER, target.id.as_str(), 10)
        .await
        .expect("list rolled recalls");
    assert_eq!(recalls.len(), 2);
    let rollup = recalls
        .iter()
        .find(|record| record.recall_key == "rollup")
        .expect("generated rollup");
    assert_eq!(rollup.recall_text, "Rolled project memory.");
    assert_eq!(rollup.level, 1);

    provider_task.abort();
    database.close().await;
    fs::remove_dir_all(root).expect("cleanup memory rollup database");
}
