// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::fs;

use uuid::Uuid;

use super::*;
use crate::local_runtime::storage::{
    BeginLocalTurnInput, CreateLocalSessionInput, UpsertLocalProjectInput,
};

#[tokio::test]
async fn persists_and_resolves_prompt_once() {
    let root = std::env::temp_dir().join(format!("chatos-local-ask-user-{}", Uuid::new_v4()));
    let database = LocalDatabase::open(root.join("runtime.sqlite3"))
        .await
        .expect("open local database");
    let (session_id, turn_id) = seed_turn(&database).await;
    let now = local_now_rfc3339();
    let record = LocalAskUserPromptRecord {
        id: "up_local_storage".to_string(),
        session_id: session_id.clone(),
        turn_id,
        owner_user_id: "user-ask".to_string(),
        tool_call_id: None,
        kind: "choice".to_string(),
        status: "pending".to_string(),
        prompt_json: r#"{"prompt_id":"up_local_storage"}"#.to_string(),
        response_json: None,
        expires_at: None,
        created_at: now.clone(),
        updated_at: now,
    };
    database
        .create_ask_user_prompt(&record)
        .await
        .expect("create prompt");

    let pending = database
        .list_ask_user_prompts("user-ask", session_id.as_str(), true, 10)
        .await
        .expect("list prompts");
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].status, "pending");

    let resolved = database
        .resolve_ask_user_prompt(
            "user-ask",
            record.id.as_str(),
            "ok",
            r#"{"status":"ok","selection":"yes"}"#,
        )
        .await
        .expect("resolve prompt")
        .expect("resolved record");
    assert_eq!(resolved.status, "ok");
    assert!(database
        .resolve_ask_user_prompt("user-ask", record.id.as_str(), "canceled", "{}")
        .await
        .expect("second resolution")
        .is_none());

    database.close().await;
    fs::remove_dir_all(root).expect("cleanup local database");
}

async fn seed_turn(database: &LocalDatabase) -> (String, String) {
    database
        .upsert_project(UpsertLocalProjectInput {
            project_id: "project-ask".to_string(),
            owner_user_id: "user-ask".to_string(),
            device_id: "device-ask".to_string(),
            workspace_id: "workspace-ask".to_string(),
            project_name: "Ask User project".to_string(),
            root_relative_path: None,
        })
        .await
        .expect("upsert project");
    let session = database
        .create_session(CreateLocalSessionInput {
            project_id: "project-ask".to_string(),
            owner_user_id: "user-ask".to_string(),
            title: "Ask User session".to_string(),
            selected_model_id: None,
            selected_agent_id: None,
        })
        .await
        .expect("create session");
    let turn_id = "lc_turn_ask_storage".to_string();
    database
        .begin_turn(BeginLocalTurnInput {
            session_id: session.id.clone(),
            owner_user_id: "user-ask".to_string(),
            turn_id: turn_id.clone(),
            idempotency_key: "ask-storage".to_string(),
            content: "ask".to_string(),
            metadata_json: None,
        })
        .await
        .expect("begin turn");
    (session.id, turn_id)
}
