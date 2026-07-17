// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::fs;
use std::sync::Arc;

use axum::routing::post;
use axum::{Json, Router};
use serde_json::json;
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::local_runtime::storage::{
    BeginLocalTurnInput, CompleteLocalTurnInput, CreateLocalSessionInput, LocalDatabase,
    UpsertLocalProjectInput,
};
use crate::{LocalRuntime, LocalState};

use super::maybe_spawn_local_memory_review;

#[tokio::test]
async fn automatically_generates_local_summary_after_threshold() {
    let provider = Router::new().route(
        "/responses",
        post(|| async {
            Json(json!({
                "id": "summary-response-1",
                "status": "completed",
                "output_text": "The user asked the client to keep memory local.",
                "output": [{
                    "type": "message",
                    "role": "assistant",
                    "content": [{
                        "type": "output_text",
                        "text": "The user asked the client to keep memory local."
                    }]
                }]
            }))
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind mock memory model");
    let provider_url = format!(
        "http://{}",
        listener.local_addr().expect("provider address")
    );
    let provider_task = tokio::spawn(async move {
        let _ = axum::serve(listener, provider).await;
    });

    let root = std::env::temp_dir().join(format!("chatos-local-memory-run-{}", Uuid::new_v4()));
    let database = LocalDatabase::open(root.join("runtime.sqlite3"))
        .await
        .expect("open local database");
    database
        .upsert_project(UpsertLocalProjectInput {
            project_id: "project-memory-run".to_string(),
            owner_user_id: "user-memory".to_string(),
            device_id: "device-memory".to_string(),
            workspace_id: "workspace-memory".to_string(),
            project_name: "Memory runtime project".to_string(),
            root_relative_path: None,
        })
        .await
        .expect("upsert project");
    let session = database
        .create_session(CreateLocalSessionInput {
            project_id: "project-memory-run".to_string(),
            owner_user_id: "user-memory".to_string(),
            title: "Memory runtime session".to_string(),
            selected_model_id: Some("model-memory".to_string()),
            selected_agent_id: None,
        })
        .await
        .expect("create session");
    database
        .begin_turn(BeginLocalTurnInput {
            session_id: session.id.clone(),
            owner_user_id: "user-memory".to_string(),
            turn_id: "lc_turn_memory_run".to_string(),
            idempotency_key: "memory-run".to_string(),
            content: "Keep memory on this device. ".repeat(1_300),
            metadata_json: None,
        })
        .await
        .expect("begin turn");
    database
        .complete_turn(CompleteLocalTurnInput {
            turn_id: "lc_turn_memory_run".to_string(),
            owner_user_id: "user-memory".to_string(),
            content: "Understood".to_string(),
            reasoning: None,
            tool_calls_json: None,
            metadata_json: None,
        })
        .await
        .expect("complete turn");
    let state = serde_json::from_value::<LocalState>(json!({
        "auth": {
            "cloud_base_url": "https://cloud.example.invalid",
            "user_service_base_url": "https://users.example.invalid",
            "access_token": "token",
            "device_name": "Test device",
            "user": {
                "id": "user-memory",
                "username": "tester",
                "display_name": "Tester",
                "role": "user"
            }
        },
        "device_id": "device-memory",
        "workspaces": [{
            "id": "workspace-memory",
            "absolute_root": root.to_string_lossy(),
            "alias": "memory-workspace",
            "fingerprint": "memory-workspace-fingerprint"
        }],
        "model_configs": {
            "configs": [{
                "id": "model-memory",
                "name": "Local memory model",
                "provider": "openai",
                "model": "gpt-memory-test",
                "base_url": provider_url,
                "api_key": "device-secret",
                "enabled": true,
                "supports_images": false,
                "supports_reasoning": false,
                "supports_responses": true,
                "created_at": "2026-01-01T00:00:00Z",
                "updated_at": "2026-01-01T00:00:00Z"
            }],
            "settings": {}
        }
    }))
    .expect("build local state");
    let runtime = LocalRuntime::new(
        root.join("state.json"),
        Arc::new(RwLock::new(state)),
        reqwest::Client::new(),
        database.clone(),
    );

    let started = maybe_spawn_local_memory_review(&runtime, "user-memory", session.id.as_str())
        .await
        .expect("start automatic local memory review");
    assert!(started);
    tokio::time::timeout(std::time::Duration::from_secs(3), async {
        while runtime.memory_jobs.is_running(session.id.as_str()) {
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("automatic memory review completion");
    let summaries = database
        .list_memory_summaries("user-memory", session.id.as_str(), 10, 0)
        .await
        .expect("list summaries");
    assert_eq!(summaries.len(), 1);
    assert_eq!(
        summaries[0].summary_text,
        "The user asked the client to keep memory local."
    );
    assert_eq!(summaries[0].summary_model, "gpt-memory-test");
    assert_eq!(summaries[0].trigger_type, "automatic_threshold");
    let recall_session = database
        .create_session(CreateLocalSessionInput {
            project_id: "project-memory-run".to_string(),
            owner_user_id: "user-memory".to_string(),
            title: "Recall target session".to_string(),
            selected_model_id: Some("model-memory".to_string()),
            selected_agent_id: None,
        })
        .await
        .expect("create recall target session");
    let recalls = database
        .list_subject_memories_for_session("user-memory", recall_session.id.as_str(), 10)
        .await
        .expect("list generated project recall");
    assert_eq!(recalls.len(), 1);
    assert_eq!(recalls[0].subject_type, "project");
    assert_eq!(recalls[0].source_session_id, session.id);

    provider_task.abort();
    database.close().await;
    fs::remove_dir_all(root).expect("cleanup local memory runtime database");
}
