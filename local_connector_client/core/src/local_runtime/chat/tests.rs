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
    CreateLocalSessionInput, LocalDatabase, UpsertLocalProjectInput,
};
use crate::local_runtime::{
    LOCAL_UNSCOPED_PROJECT_ID, LOCAL_UNSCOPED_PROJECT_NAME, LOCAL_UNSCOPED_WORKSPACE_ID,
};
use crate::{LocalRuntime, LocalState};

use super::{execute_chat_turn, LocalChatSendRequest};

pub(in crate::local_runtime) mod capability_support;
mod tool_execution;
mod turn_control;

#[tokio::test]
async fn prepares_non_project_contact_tools_in_a_private_local_workspace() {
    let root = std::env::temp_dir().join(format!("chatos-local-contact-{}", Uuid::new_v4()));
    let database = LocalDatabase::open(root.join("runtime.sqlite3"))
        .await
        .expect("open local database");
    capability_support::seed_chat_capabilities(&database, "user-1")
        .await
        .expect("seed chat capabilities");
    let project = database
        .upsert_project(UpsertLocalProjectInput {
            project_id: LOCAL_UNSCOPED_PROJECT_ID.to_string(),
            owner_user_id: "user-1".to_string(),
            device_id: "device-1".to_string(),
            workspace_id: LOCAL_UNSCOPED_WORKSPACE_ID.to_string(),
            project_name: LOCAL_UNSCOPED_PROJECT_NAME.to_string(),
            root_relative_path: None,
        })
        .await
        .expect("upsert local unscoped project");
    let session = database
        .create_session_with_contact(
            CreateLocalSessionInput {
                project_id: project.project_id.clone(),
                owner_user_id: "user-1".to_string(),
                title: "Local contact".to_string(),
                selected_model_id: None,
                selected_agent_id: Some("agent-1".to_string()),
            },
            Some("contact-1".to_string()),
        )
        .await
        .expect("create local contact session");
    let settings = database
        .get_runtime_settings("user-1", session.id.as_str())
        .await
        .expect("load runtime settings")
        .expect("runtime settings");
    let state = serde_json::from_value::<LocalState>(json!({
        "device_id": "device-1",
        "workspaces": []
    }))
    .expect("build local state");
    let runtime = LocalRuntime::new(
        root.join("state.json"),
        Arc::new(RwLock::new(state)),
        reqwest::Client::new(),
        database.clone(),
    );

    let prepared = super::tools::prepare_local_chat_tools(
        &runtime,
        "user-1",
        "request-contact-1",
        &project,
        &settings,
        chatos_plugin_management_sdk::SystemAgentKey::ChatosConversationAgent,
        false,
    )
    .await
    .expect("prepare local contact tools");

    assert_eq!(
        prepared
            .project_root
            .canonicalize()
            .expect("canonical prepared root"),
        root.join("unscoped-workspace")
            .canonicalize()
            .expect("canonical expected root")
    );
    assert!(prepared.project_root.is_dir());
    assert!(session.id.starts_with("lc_session_"));
    assert!(prepared.available_tools.iter().any(|tool| {
        tool.get("name").and_then(serde_json::Value::as_str)
            == Some("task_runner_service_create_task")
    }));

    database.close().await;
    fs::remove_dir_all(root).expect("cleanup local contact database");
}

#[tokio::test]
async fn executes_text_chat_with_device_model_and_persists_both_messages() {
    let provider = Router::new().route(
        "/responses",
        post(|| async {
            Json(json!({
                "id": "response-1",
                "status": "completed",
                "output_text": "local reply",
                "output": [{
                    "type": "message",
                    "role": "assistant",
                    "content": [{"type": "output_text", "text": "local reply"}]
                }]
            }))
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind mock model provider");
    let provider_url = format!(
        "http://{}",
        listener.local_addr().expect("provider address")
    );
    let provider_task = tokio::spawn(async move {
        let _ = axum::serve(listener, provider).await;
    });

    let root = std::env::temp_dir().join(format!("chatos-local-chat-{}", Uuid::new_v4()));
    let database = LocalDatabase::open(root.join("runtime.sqlite3"))
        .await
        .expect("open local database");
    capability_support::seed_chat_capabilities(&database, "user-1")
        .await
        .expect("seed chat capabilities");
    database
        .upsert_project(UpsertLocalProjectInput {
            project_id: "project-1".to_string(),
            owner_user_id: "user-1".to_string(),
            device_id: "device-1".to_string(),
            workspace_id: "workspace-1".to_string(),
            project_name: "Local project".to_string(),
            root_relative_path: None,
        })
        .await
        .expect("upsert local project");
    let session = database
        .create_session(CreateLocalSessionInput {
            project_id: "project-1".to_string(),
            owner_user_id: "user-1".to_string(),
            title: "Local session".to_string(),
            selected_model_id: Some("model-1".to_string()),
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
            "id": "workspace-1",
            "absolute_root": root.to_string_lossy(),
            "alias": "test-workspace",
            "fingerprint": "test-workspace-fingerprint"
        }],
        "model_configs": {
            "configs": [{
                "id": "model-1",
                "name": "Local model",
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
            content: "hello".to_string(),
            turn_id: Some("lc_turn_1".to_string()),
            idempotency_key: Some("request-1".to_string()),
            model_config_id: Some("model-1".to_string()),
            reasoning_enabled: Some(false),
            system_prompt: Some("Answer briefly".to_string()),
            attachments: Vec::new(),
            ai_model_config: Default::default(),
        },
    )
    .await
    .expect("execute local chat turn");
    assert!(!result.reused);
    assert_eq!(
        result
            .snapshot
            .assistant_message
            .as_ref()
            .map(|message| message.content.as_str()),
        Some("local reply")
    );
    let messages = database
        .list_messages("user-1", session.id.as_str())
        .await
        .expect("list persisted messages");
    assert_eq!(messages.len(), 2);
    assert_eq!(messages[0].content, "hello");
    assert_eq!(messages[1].content, "local reply");
    let events = database
        .list_runtime_events("user-1", session.id.as_str(), Some("lc_turn_1"), 0, 100)
        .await
        .expect("list local text events");
    assert!(events.iter().any(
        |event| event.event_name == "chat.chunk" && event.payload_json.contains("local reply")
    ));
    assert_eq!(
        events.last().map(|event| event.event_name.as_str()),
        Some("chat.completed")
    );

    provider_task.abort();
    database.close().await;
    fs::remove_dir_all(root).expect("cleanup local chat database");
}
