// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use axum::extract::State;
use axum::routing::post;
use axum::{Json, Router};
use serde_json::json;
use tokio::sync::RwLock;

use crate::local_runtime::storage::{
    CreateLocalMemorySummaryInput, CreateLocalSessionInput, LocalDatabase,
};
use crate::{LocalRuntime, LocalState};

pub(super) const OWNER: &str = "user-memory-rollup";
pub(super) const PROJECT: &str = "project-memory-rollup";

pub(super) async fn start_provider() -> (String, Arc<AtomicUsize>, tokio::task::JoinHandle<()>) {
    let calls = Arc::new(AtomicUsize::new(0));
    let provider = Router::new()
        .route(
            "/responses",
            post(|State(calls): State<Arc<AtomicUsize>>| async move {
                let text = if calls.fetch_add(1, Ordering::SeqCst) == 0 {
                    "Current session summary."
                } else {
                    "Rolled project memory."
                };
                Json(json!({
                    "id": "memory-rollup-response",
                    "status": "completed",
                    "output_text": text,
                    "output": [{
                        "type": "message",
                        "role": "assistant",
                        "content": [{ "type": "output_text", "text": text }]
                    }]
                }))
            }),
        )
        .with_state(calls.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind mock rollup model");
    let url = format!(
        "http://{}",
        listener.local_addr().expect("provider address")
    );
    let task = tokio::spawn(async move {
        let _ = axum::serve(listener, provider).await;
    });
    (url, calls, task)
}

pub(super) async fn seed_recall(database: &LocalDatabase, index: usize) {
    let session = database
        .create_session(CreateLocalSessionInput {
            project_id: PROJECT.to_string(),
            owner_user_id: OWNER.to_string(),
            title: format!("Seed memory {index}"),
            selected_model_id: Some("model-memory-rollup".to_string()),
            selected_agent_id: None,
        })
        .await
        .expect("create seed session");
    let summary = database
        .create_memory_summary(CreateLocalMemorySummaryInput {
            owner_user_id: OWNER.to_string(),
            session_id: session.id.clone(),
            summary_text: format!("Seed durable memory {index}"),
            summary_model: "test-model".to_string(),
            trigger_type: "test".to_string(),
            source_start_message_id: None,
            source_end_message_id: None,
            source_message_count: 0,
            source_estimated_tokens: 1,
            level: 0,
        })
        .await
        .expect("create seed summary");
    database
        .upsert_subject_memories_for_summary(OWNER, &session, &summary)
        .await
        .expect("create seed recall");
}

pub(super) fn test_runtime(
    root: &std::path::Path,
    provider_url: String,
    database: LocalDatabase,
) -> LocalRuntime {
    let state = serde_json::from_value::<LocalState>(json!({
        "auth": {
            "cloud_base_url": "https://cloud.example.invalid",
            "user_service_base_url": "https://users.example.invalid",
            "access_token": "token",
            "device_name": "Test device",
            "user": { "id": OWNER, "username": "tester", "display_name": "Tester", "role": "user" }
        },
        "device_id": "device-memory-rollup",
        "workspaces": [{
            "id": "workspace-memory-rollup",
            "absolute_root": root.to_string_lossy(),
            "alias": "memory-rollup-workspace",
            "fingerprint": "memory-rollup-fingerprint"
        }],
        "model_configs": {
            "configs": [{
                "id": "model-memory-rollup",
                "name": "Local memory rollup model",
                "provider": "openai",
                "model": "gpt-memory-rollup-test",
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
    LocalRuntime::new(
        root.join("state.json"),
        Arc::new(RwLock::new(state)),
        reqwest::Client::new(),
        database,
    )
}
