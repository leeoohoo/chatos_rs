// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::VecDeque;
use std::fs;
use std::sync::Arc;

use axum::extract::State;
use axum::routing::post;
use axum::{Json, Router};
use serde_json::{json, Value};
use tokio::sync::{Mutex, RwLock};
use uuid::Uuid;

use crate::local_runtime::storage::{
    CreateLocalSessionInput, LocalDatabase, UpsertLocalProjectInput,
};
use crate::{LocalRuntime, LocalState};

use super::super::{execute_chat_turn, LocalChatSendRequest};

#[derive(Clone)]
struct MockModelProvider {
    responses: Arc<Mutex<VecDeque<Value>>>,
    requests: Arc<Mutex<Vec<Value>>>,
}

async fn respond(
    State(provider): State<MockModelProvider>,
    Json(request): Json<Value>,
) -> Json<Value> {
    provider.requests.lock().await.push(request);
    Json(
        provider
            .responses
            .lock()
            .await
            .pop_front()
            .unwrap_or_else(|| json!({"id":"response-empty","status":"completed"})),
    )
}

#[tokio::test]
async fn executes_local_file_tool_and_persists_process_messages() {
    let requests = Arc::new(Mutex::new(Vec::new()));
    let provider = MockModelProvider {
        responses: Arc::new(Mutex::new(VecDeque::from([
            json!({
                "id": "response-tool",
                "status": "completed",
                "output": [{
                    "type": "function_call",
                    "call_id": "call-read-1",
                    "name": "code_maintainer_read_read_file_raw",
                    "arguments": "{\"path\":\"facts.txt\",\"with_line_numbers\":false}"
                }]
            }),
            json!({
                "id": "response-final",
                "status": "completed",
                "output_text": "I found the local fact.",
                "output": [{
                    "type": "message",
                    "role": "assistant",
                    "content": [{"type": "output_text", "text": "I found the local fact."}]
                }]
            }),
        ]))),
        requests: requests.clone(),
    };
    let app = Router::new()
        .route("/responses", post(respond))
        .with_state(provider);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind mock model provider");
    let provider_url = format!(
        "http://{}",
        listener.local_addr().expect("provider address")
    );
    let provider_task = tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });

    let root = std::env::temp_dir().join(format!("chatos-local-tools-{}", Uuid::new_v4()));
    fs::create_dir_all(root.as_path()).expect("create local workspace");
    fs::write(root.join("facts.txt"), "local tool content\n").expect("write local fact");
    let database = LocalDatabase::open(root.join("runtime.sqlite3"))
        .await
        .expect("open local database");
    super::capability_support::seed_chat_capabilities(&database, "user-1")
        .await
        .expect("seed tool capabilities");
    database
        .upsert_project(UpsertLocalProjectInput {
            project_id: "project-tools".to_string(),
            owner_user_id: "user-1".to_string(),
            device_id: "device-1".to_string(),
            workspace_id: "workspace-tools".to_string(),
            project_name: "Local tools project".to_string(),
            root_relative_path: None,
        })
        .await
        .expect("upsert local project");
    let session = database
        .create_session(CreateLocalSessionInput {
            project_id: "project-tools".to_string(),
            owner_user_id: "user-1".to_string(),
            title: "Local tool session".to_string(),
            selected_model_id: Some("model-tools".to_string()),
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
            "id": "workspace-tools",
            "absolute_root": root.to_string_lossy(),
            "alias": "tools-workspace",
            "fingerprint": "tools-workspace-fingerprint"
        }],
        "model_configs": {
            "configs": [{
                "id": "model-tools",
                "name": "Local tool model",
                "provider": "openai",
                "prompt_vendor": "gpt",
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

    let result = execute_chat_turn(
        &runtime,
        "user-1",
        LocalChatSendRequest {
            conversation_id: session.id.clone(),
            content: "Read the local fact".to_string(),
            turn_id: Some("lc_turn_tools_1".to_string()),
            idempotency_key: Some("request-tools-1".to_string()),
            model_config_id: Some("model-tools".to_string()),
            reasoning_enabled: Some(false),
            system_prompt: None,
            attachments: Vec::new(),
            ai_model_config: Default::default(),
        },
    )
    .await
    .expect("execute local tool chat");

    assert_eq!(result.process_messages.len(), 2);
    assert_eq!(result.process_messages[0].role, "assistant");
    assert!(result.process_messages[0]
        .tool_calls_json
        .as_deref()
        .is_some_and(|value| value.contains("code_maintainer_read_read_file_raw")));
    assert_eq!(result.process_messages[1].role, "tool");
    assert_eq!(
        result.process_messages[1].tool_call_id.as_deref(),
        Some("call-read-1")
    );
    assert!(result.process_messages[1]
        .metadata_json
        .as_deref()
        .is_some_and(|value| value.contains("local tool content")));
    assert_eq!(
        result
            .snapshot
            .assistant_message
            .as_ref()
            .map(|message| message.content.as_str()),
        Some("I found the local fact.")
    );

    let messages = database
        .list_messages("user-1", session.id.as_str())
        .await
        .expect("list local tool messages");
    assert_eq!(messages.len(), 4);
    let events = database
        .list_runtime_events(
            "user-1",
            session.id.as_str(),
            Some("lc_turn_tools_1"),
            0,
            100,
        )
        .await
        .expect("list local tool events");
    assert!(events
        .windows(2)
        .all(|pair| pair[0].event_seq < pair[1].event_seq));
    let event_names = events
        .iter()
        .map(|event| event.event_name.as_str())
        .collect::<Vec<_>>();
    assert!(event_names.contains(&"chat.tools.start"));
    assert!(event_names.contains(&"chat.tools.stream"));
    assert!(event_names.contains(&"chat.tools.end"));
    assert_eq!(event_names.last().copied(), Some("chat.completed"));
    let captured = requests.lock().await.clone();
    assert_eq!(captured.len(), 2);
    assert!(captured[0]
        .get("tools")
        .and_then(Value::as_array)
        .is_some_and(|tools| tools.iter().any(|tool| {
            tool.get("name").and_then(Value::as_str) == Some("code_maintainer_read_read_file_raw")
        })));
    assert!(captured[1].to_string().contains("local tool content"));

    provider_task.abort();
    database.close().await;
    fs::remove_dir_all(root).expect("cleanup local tool database");
}
