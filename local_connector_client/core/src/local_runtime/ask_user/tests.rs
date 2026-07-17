// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::fs;
use std::time::Duration;

use chatos_builtin_tools::{AskUserPromptPayload, AskUserResponseSubmission, AskUserStore};
use serde_json::json;
use uuid::Uuid;

use super::registry::LocalAskUserPromptRegistry;
use super::store::LocalAskUserStore;
use crate::local_runtime::storage::{
    BeginLocalTurnInput, CreateLocalSessionInput, LocalDatabase, UpsertLocalProjectInput,
};

#[tokio::test(flavor = "multi_thread")]
async fn waits_for_sqlite_prompt_resolution_without_cloud_fallback() {
    let root = std::env::temp_dir().join(format!("chatos-local-ask-store-{}", Uuid::new_v4()));
    let database = LocalDatabase::open(root.join("runtime.sqlite3"))
        .await
        .expect("open local database");
    let (session_id, turn_id) = seed_turn(&database).await;
    let registry = LocalAskUserPromptRegistry::default();
    let store = LocalAskUserStore::new(database.clone(), "user-ask", registry.clone());
    let prompt_id = "up_local_wait".to_string();
    let task_prompt_id = prompt_id.clone();
    let task = tokio::spawn(async move {
        store
            .execute_prompt(
                AskUserPromptPayload {
                    prompt_id: task_prompt_id,
                    conversation_id: session_id,
                    conversation_turn_id: turn_id,
                    tool_call_id: None,
                    kind: "choice".to_string(),
                    title: "Choose".to_string(),
                    message: "Continue?".to_string(),
                    allow_cancel: true,
                    timeout_ms: 2_000,
                    payload: json!({ "choice": { "options": [{ "value": "yes" }] } }),
                },
                None,
            )
            .await
    });

    wait_until_persisted(&database, prompt_id.as_str()).await;
    let response = AskUserResponseSubmission {
        status: "ok".to_string(),
        values: None,
        selection: Some(json!("yes")),
        reason: None,
    };
    database
        .resolve_ask_user_prompt(
            "user-ask",
            prompt_id.as_str(),
            "ok",
            serde_json::to_string(&response)
                .expect("serialize response")
                .as_str(),
        )
        .await
        .expect("resolve prompt")
        .expect("pending prompt");
    registry.notify(prompt_id.as_str()).await;

    let decision = tokio::time::timeout(Duration::from_secs(2), task)
        .await
        .expect("store completed")
        .expect("task joined")
        .expect("prompt decision");
    assert_eq!(decision.status, "ok");
    assert_eq!(decision.response.selection, Some(json!("yes")));

    database.close().await;
    fs::remove_dir_all(root).expect("cleanup local database");
}

async fn wait_until_persisted(database: &LocalDatabase, prompt_id: &str) {
    for _ in 0..50 {
        if database
            .get_ask_user_prompt("user-ask", prompt_id)
            .await
            .expect("read prompt")
            .is_some()
        {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("prompt was not persisted");
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
    let turn_id = "lc_turn_ask_wait".to_string();
    database
        .begin_turn(BeginLocalTurnInput {
            session_id: session.id.clone(),
            owner_user_id: "user-ask".to_string(),
            turn_id: turn_id.clone(),
            idempotency_key: "ask-wait".to_string(),
            content: "ask".to_string(),
            metadata_json: None,
        })
        .await
        .expect("begin turn");
    (session.id, turn_id)
}
