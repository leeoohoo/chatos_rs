// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::fs;

use uuid::Uuid;

use super::super::{
    CreateLocalMemorySummaryInput, CreateLocalSessionInput, LocalDatabase,
    SaveLocalSubjectMemoryRollupInput, UpsertLocalProjectInput,
};

const OWNER: &str = "user-rollup";
const PROJECT: &str = "project-rollup";

#[tokio::test]
async fn compacts_old_recalls_into_a_cumulative_rollup() {
    let root = std::env::temp_dir().join(format!("chatos-recall-rollup-{}", Uuid::new_v4()));
    let database = LocalDatabase::open(root.join("runtime.sqlite3"))
        .await
        .expect("open local database");
    database
        .upsert_project(UpsertLocalProjectInput {
            project_id: PROJECT.to_string(),
            owner_user_id: OWNER.to_string(),
            device_id: "device-rollup".to_string(),
            workspace_id: "workspace-rollup".to_string(),
            project_name: "Rollup project".to_string(),
            root_relative_path: None,
        })
        .await
        .expect("upsert project");
    for index in 0..4 {
        create_project_recall(&database, index).await;
    }

    let first_plan = database
        .prepare_subject_memory_rollup(OWNER, "project", PROJECT, PROJECT, 2)
        .await
        .expect("prepare first rollup")
        .expect("first rollup plan");
    assert!(first_plan.existing_rollup.is_none());
    assert_eq!(first_plan.candidates.len(), 3);
    save_plan(&database, first_plan, "first cumulative rollup", 1).await;
    assert_eq!(subject_memory_count(&database).await, 2);

    create_project_recall(&database, 4).await;
    let second_plan = database
        .prepare_subject_memory_rollup(OWNER, "project", PROJECT, PROJECT, 2)
        .await
        .expect("prepare second rollup")
        .expect("second rollup plan");
    assert!(second_plan.existing_rollup.is_some());
    assert_eq!(second_plan.candidates.len(), 1);
    save_plan(&database, second_plan, "second cumulative rollup", 2).await;

    let rollup = database
        .get_subject_memory(OWNER, "project", PROJECT, PROJECT, "rollup")
        .await
        .expect("load rollup")
        .expect("persisted rollup");
    assert_eq!(rollup.recall_text, "second cumulative rollup");
    assert_eq!(rollup.level, 2);
    assert_eq!(subject_memory_count(&database).await, 2);

    let target = database
        .create_session(CreateLocalSessionInput {
            project_id: PROJECT.to_string(),
            owner_user_id: OWNER.to_string(),
            title: "Rollup forget target".to_string(),
            selected_model_id: Some("model-rollup".to_string()),
            selected_agent_id: None,
        })
        .await
        .expect("create rollup target");
    let deleted = database
        .forget_subject_memory_for_session(OWNER, target.id.as_str(), rollup.id.as_str())
        .await
        .expect("forget rollup");
    assert_eq!(deleted, 1);
    assert_eq!(subject_memory_count(&database).await, 1);
    let marker_count = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM subject_memory_forget_markers WHERE owner_user_id = ?",
    )
    .bind(OWNER)
    .fetch_one(database.pool())
    .await
    .expect("count rollup forget markers");
    assert_eq!(marker_count, 0);

    database.close().await;
    fs::remove_dir_all(root).expect("cleanup rollup database");
}

async fn create_project_recall(database: &LocalDatabase, index: usize) {
    let session = database
        .create_session(CreateLocalSessionInput {
            project_id: PROJECT.to_string(),
            owner_user_id: OWNER.to_string(),
            title: format!("Rollup session {index}"),
            selected_model_id: Some("model-rollup".to_string()),
            selected_agent_id: None,
        })
        .await
        .expect("create rollup session");
    let summary = database
        .create_memory_summary(CreateLocalMemorySummaryInput {
            owner_user_id: OWNER.to_string(),
            session_id: session.id.clone(),
            summary_text: format!("Durable recall {index}"),
            summary_model: "test-model".to_string(),
            trigger_type: "test".to_string(),
            source_start_message_id: None,
            source_end_message_id: None,
            source_message_count: 0,
            source_estimated_tokens: 1,
            level: 0,
        })
        .await
        .expect("create rollup summary");
    database
        .upsert_subject_memories_for_summary(OWNER, &session, &summary)
        .await
        .expect("create project recall");
}

async fn save_plan(
    database: &LocalDatabase,
    plan: super::super::LocalSubjectMemoryRollupPlan,
    recall_text: &str,
    level: i64,
) {
    let source = plan.candidates.last().expect("rollup source");
    database
        .save_subject_memory_rollup(SaveLocalSubjectMemoryRollupInput {
            owner_user_id: OWNER.to_string(),
            subject_type: "project".to_string(),
            subject_id: PROJECT.to_string(),
            project_id: PROJECT.to_string(),
            recall_text: recall_text.to_string(),
            source_session_id: source.source_session_id.clone(),
            source_summary_id: source.source_summary_id.clone(),
            level,
            candidate_ids: plan
                .candidates
                .iter()
                .map(|candidate| candidate.id.clone())
                .collect(),
        })
        .await
        .expect("save rollup plan");
}

async fn subject_memory_count(database: &LocalDatabase) -> i64 {
    sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM subject_memories WHERE owner_user_id = ? AND project_id = ?",
    )
    .bind(OWNER)
    .bind(PROJECT)
    .fetch_one(database.pool())
    .await
    .expect("count subject memories")
}
