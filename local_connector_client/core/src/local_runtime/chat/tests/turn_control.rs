// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::VecDeque;
use std::fs;
use std::sync::Arc;

use axum::extract::State;
use axum::routing::post;
use axum::{Json, Router};
use serde_json::{json, Value};
use tokio::sync::{Mutex, Notify, RwLock};
use uuid::Uuid;

use crate::local_now_rfc3339;
use crate::local_runtime::chat::LocalRuntimeGuidance;
use crate::local_runtime::storage::{
    AppendLocalMessageInput, CreateLocalSessionInput, LocalDatabase, UpsertLocalProjectInput,
};
use crate::{LocalRuntime, LocalState};

use super::super::{execute_chat_turn, LocalChatExecutionErrorKind, LocalChatSendRequest};

#[derive(Clone)]
struct ControlledProvider {
    responses: Arc<Mutex<VecDeque<Value>>>,
    requests: Arc<Mutex<Vec<Value>>>,
    first_request_seen: Arc<Notify>,
    release_first_response: Arc<Notify>,
    hold_first_response: bool,
}

async fn respond(
    State(provider): State<ControlledProvider>,
    Json(request): Json<Value>,
) -> Json<Value> {
    let request_index = {
        let mut requests = provider.requests.lock().await;
        requests.push(request);
        requests.len()
    };
    if request_index == 1 {
        provider.first_request_seen.notify_one();
        if provider.hold_first_response {
            provider.release_first_response.notified().await;
        }
    }
    Json(
        provider
            .responses
            .lock()
            .await
            .pop_front()
            .unwrap_or_else(|| json!({"id":"response-empty","status":"completed"})),
    )
}

struct LocalControlHarness {
    root: std::path::PathBuf,
    database: LocalDatabase,
    runtime: LocalRuntime,
    session_id: String,
    provider: ControlledProvider,
    provider_task: tokio::task::JoinHandle<()>,
}

impl LocalControlHarness {
    async fn new(responses: Vec<Value>, hold_first_response: bool) -> Self {
        let provider = ControlledProvider {
            responses: Arc::new(Mutex::new(responses.into_iter().collect())),
            requests: Arc::new(Mutex::new(Vec::new())),
            first_request_seen: Arc::new(Notify::new()),
            release_first_response: Arc::new(Notify::new()),
            hold_first_response,
        };
        let app = Router::new()
            .route("/responses", post(respond))
            .with_state(provider.clone());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind controlled model provider");
        let provider_url = format!(
            "http://{}",
            listener.local_addr().expect("provider address")
        );
        let provider_task = tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });

        let root = std::env::temp_dir().join(format!("chatos-local-control-{}", Uuid::new_v4()));
        fs::create_dir_all(root.as_path()).expect("create control workspace");
        let database = LocalDatabase::open(root.join("runtime.sqlite3"))
            .await
            .expect("open local database");
        super::capability_support::seed_chat_capabilities(&database, "user-1")
            .await
            .expect("seed control capabilities");
        database
            .upsert_project(UpsertLocalProjectInput {
                project_id: "project-control".to_string(),
                owner_user_id: "user-1".to_string(),
                device_id: "device-1".to_string(),
                workspace_id: "workspace-control".to_string(),
                project_name: "Local control project".to_string(),
                root_relative_path: None,
            })
            .await
            .expect("upsert local project");
        let session = database
            .create_session(CreateLocalSessionInput {
                project_id: "project-control".to_string(),
                owner_user_id: "user-1".to_string(),
                title: "Local control session".to_string(),
                selected_model_id: Some("model-control".to_string()),
                selected_agent_id: None,
            })
            .await
            .expect("create local session");
        let state = serde_json::from_value::<LocalState>(json!({
            "auth": {
                "cloud_base_url": "https://cloud.example.invalid",
                "user_service_base_url": "https://users.example.invalid",
                "access_token": "token",
                "device_name": "Test device",
                "user": {
                    "id": "user-1",
                    "username": "tester",
                    "display_name": "Tester",
                    "role": "user"
                }
            },
            "device_id": "device-1",
            "workspaces": [{
                "id": "workspace-control",
                "absolute_root": root.to_string_lossy(),
                "alias": "control-workspace",
                "fingerprint": "control-workspace-fingerprint"
            }],
            "model_configs": {
                "configs": [{
                    "id": "model-control",
                    "name": "Local control model",
                    "provider": "openai",
                    "model": "gpt-test",
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
        Self {
            root,
            database,
            runtime,
            session_id: session.id,
            provider,
            provider_task,
        }
    }

    fn request(&self, turn_id: &str) -> LocalChatSendRequest {
        LocalChatSendRequest {
            conversation_id: self.session_id.clone(),
            content: "Start local work".to_string(),
            turn_id: Some(turn_id.to_string()),
            idempotency_key: Some(format!("request-{turn_id}")),
            model_config_id: Some("model-control".to_string()),
            reasoning_enabled: Some(false),
            system_prompt: None,
            attachments: Vec::new(),
            ai_model_config: Default::default(),
        }
    }

    async fn cleanup(self) {
        self.provider.release_first_response.notify_waiters();
        self.provider_task.abort();
        self.database.close().await;
        fs::remove_dir_all(self.root).expect("cleanup control database");
    }
}

#[tokio::test]
async fn cancels_an_inflight_local_model_request() {
    let harness = LocalControlHarness::new(
        vec![json!({
            "id": "response-never-used",
            "status": "completed",
            "output_text": "too late"
        })],
        true,
    )
    .await;
    let turn_id = "lc_turn_cancel_1";
    let runtime = harness.runtime.clone();
    let request = harness.request(turn_id);
    let execution =
        tokio::spawn(async move { execute_chat_turn(&runtime, "user-1", request).await });

    tokio::time::timeout(
        std::time::Duration::from_secs(3),
        harness.provider.first_request_seen.notified(),
    )
    .await
    .expect("model request should start");
    harness
        .database
        .request_turn_cancel("user-1", harness.session_id.as_str(), Some(turn_id))
        .await
        .expect("persist cancel request");
    assert!(harness
        .runtime
        .turn_control
        .cancel(harness.session_id.as_str(), Some(turn_id)));

    let error = tokio::time::timeout(std::time::Duration::from_secs(3), execution)
        .await
        .expect("cancelled execution should finish")
        .expect("join cancelled execution")
        .expect_err("local chat should report cancellation");
    assert!(matches!(error.kind, LocalChatExecutionErrorKind::Cancelled));
    let status = sqlx::query_scalar::<_, String>("SELECT status FROM turns WHERE id = ?")
        .bind(turn_id)
        .fetch_one(harness.database.pool())
        .await
        .expect("load cancelled turn status");
    assert_eq!(status, "cancelled");

    harness.cleanup().await;
}

#[tokio::test]
async fn applies_guidance_received_during_an_active_local_turn() {
    let harness = LocalControlHarness::new(
        vec![
            json!({
                "id": "response-draft",
                "status": "completed",
                "output_text": "draft before guidance"
            }),
            json!({
                "id": "response-guided",
                "status": "completed",
                "output_text": "final after guidance"
            }),
        ],
        true,
    )
    .await;
    let turn_id = "lc_turn_guidance_1";
    let runtime = harness.runtime.clone();
    let request = harness.request(turn_id);
    let execution =
        tokio::spawn(async move { execute_chat_turn(&runtime, "user-1", request).await });

    tokio::time::timeout(
        std::time::Duration::from_secs(3),
        harness.provider.first_request_seen.notified(),
    )
    .await
    .expect("model request should start");
    let guidance_id = "gd_test_local";
    let message_id = format!("lc_message_{}", Uuid::new_v4());
    let created_at = local_now_rfc3339();
    let guidance = LocalRuntimeGuidance {
        guidance_id: guidance_id.to_string(),
        session_id: harness.session_id.clone(),
        turn_id: turn_id.to_string(),
        message_id: message_id.clone(),
        content: "Use the newly supplied preference".to_string(),
        status: "queued".to_string(),
        created_at: created_at.clone(),
    };
    harness
        .runtime
        .turn_control
        .enqueue_guidance(guidance)
        .expect("enqueue local guidance");
    harness
        .database
        .append_turn_message(AppendLocalMessageInput {
            session_id: harness.session_id.clone(),
            owner_user_id: "user-1".to_string(),
            turn_id: turn_id.to_string(),
            message_id: Some(message_id),
            role: "user".to_string(),
            content: "Use the newly supplied preference".to_string(),
            reasoning: None,
            tool_calls_json: None,
            tool_call_id: None,
            metadata_json: Some(
                json!({
                    "message_mode": "runtime_guidance",
                    "message_source": "runtime_guidance",
                    "runtime_guidance": {
                        "guidance_id": guidance_id,
                        "status": "queued"
                    }
                })
                .to_string(),
            ),
            created_at: Some(created_at),
        })
        .await
        .expect("persist local guidance message");
    harness.provider.release_first_response.notify_waiters();

    let result = tokio::time::timeout(std::time::Duration::from_secs(5), execution)
        .await
        .expect("guided execution should finish")
        .expect("join guided execution")
        .expect("guided local chat should succeed");
    assert_eq!(
        result
            .snapshot
            .assistant_message
            .as_ref()
            .map(|message| message.content.as_str()),
        Some("final after guidance")
    );
    assert!(result.process_messages.iter().any(|message| {
        message.role == "user"
            && message.content == "Use the newly supplied preference"
            && message
                .metadata_json
                .as_deref()
                .is_some_and(|metadata| metadata.contains("applied"))
    }));
    let requests = harness.provider.requests.lock().await.clone();
    assert_eq!(requests.len(), 2);
    assert!(requests[1]
        .to_string()
        .contains("Use the newly supplied preference"));

    harness.cleanup().await;
}
