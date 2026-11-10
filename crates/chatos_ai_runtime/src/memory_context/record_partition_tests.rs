// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::super::tests::{accept_thread_upsert, test_assertion_issuer, MemoryWriteServerState};
use super::*;
use axum::{extract::State, routing::put, Json, Router};
use serde_json::{json, Value};

async fn capture_batch(
    State(batches): State<Arc<Mutex<Vec<Value>>>>,
    Json(request): Json<Value>,
) -> Json<Value> {
    let count = request["records"].as_array().unwrap().len();
    batches.lock().await.push(request);
    Json(json!({"thread_id":"conversation-1","received_count":count,"upserted_count":count}))
}

fn metadata(project: Option<&str>) -> Value {
    json!({
        "tenant_id":"user-1", "agent_id":"agent-1", "thread_kind":"agent_conversation",
        "thread_project_aggregation_id":null, "memory_project_id":project
    })
}

async fn writer() -> (
    MemoryEngineRecordWriter,
    Arc<Mutex<Vec<Value>>>,
    tokio::task::JoinHandle<()>,
) {
    let batches = Arc::new(Mutex::new(vec![]));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let app = Router::new()
        .route(
            "/api/memory-engine/v1/threads/{thread_id}",
            put(accept_thread_upsert),
        )
        .with_state(MemoryWriteServerState::default())
        .merge(
            Router::new()
                .route(
                    "/api/memory-engine/v1/threads/{thread_id}/records/batch-sync",
                    put(capture_batch),
                )
                .with_state(batches.clone()),
        );
    let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let writer = MemoryEngineRecordWriter::new_direct(
        format!("http://{address}"),
        Duration::from_secs(2),
        "chatos",
        MemoryRecordScope::per_record_tenant("tenant_id"),
    )
    .unwrap()
    .with_runtime_context_assertion_issuer(test_assertion_issuer());
    (writer, batches, server)
}

fn verify_batch(batch: &Value, partition: &str) {
    assert_eq!(batch["project_partition_key"], partition);
    memory_engine_sdk::verify_runtime_context_assertion(
        batch["runtime_context_assertion"].as_str().unwrap(),
        "runtime-context-test-secret",
        "runtime-context-test-issuer",
        memory_engine_sdk::ExpectedRuntimeContext {
            audience: memory_engine_sdk::MEMORY_ENGINE_RUNTIME_CONTEXT_AUDIENCE,
            tenant_id: "user-1",
            source_id: "chatos",
            thread_id: "conversation-1",
            project_partition_key: partition,
            operation: "record_ingest",
        },
    )
    .expect("assertion must bind the record project, independently of thread aggregation");
}

#[tokio::test]
async fn stable_agent_thread_records_keep_each_current_project_partition() {
    let (writer, batches, server) = writer().await;
    for (index, project) in [Some("project-a"), Some("project-b"), None]
        .into_iter()
        .enumerate()
    {
        writer
            .save_record(
                SaveRecordInput::user_message("conversation-1", "message")
                    .with_message_id(format!("message-{index}"))
                    .with_metadata(metadata(project)),
            )
            .await
            .unwrap();
    }
    let batches = batches.lock().await;
    assert_eq!(batches.len(), 3);
    for (batch, partition) in
        batches
            .iter()
            .zip(["project:project-a", "project:project-b", "__none__"])
    {
        verify_batch(batch, partition);
    }
    server.abort();
}

#[tokio::test]
async fn tool_batches_group_by_record_project_within_one_agent_thread() {
    let (writer, batches, server) = writer().await;
    writer
        .save_tool_records(
            ["project-a", "project-b"]
                .into_iter()
                .map(|project| SaveToolRecordInput {
                    conversation_id: "conversation-1".to_string(),
                    message_id: Some(format!("tool-{project}")),
                    tool_call_id: format!("call-{project}"),
                    tool_name: "read_file".to_string(),
                    content: "result".to_string(),
                    success: true,
                    metadata: Some(metadata(Some(project))),
                    ..Default::default()
                })
                .collect(),
        )
        .await
        .unwrap();
    let batches = batches.lock().await;
    assert_eq!(batches.len(), 2);
    for (batch, partition) in batches
        .iter()
        .zip(["project:project-a", "project:project-b"])
    {
        verify_batch(batch, partition);
        assert_eq!(batch["records"].as_array().unwrap().len(), 1);
    }
    server.abort();
}

#[test]
fn task_record_partition_rejects_conflicting_or_malformed_project_identity() {
    let identity = RecordThreadIdentity {
        agent_id: Some("agent-1".to_string()),
        agent_name: None,
        project_aggregation_id: Some("project-a".to_string()),
        thread_kind: "agent_task".to_string(),
    };
    let record = |project| {
        SaveRecordInput::user_message("task-1", "message").with_metadata(metadata(project))
    };
    assert_eq!(
        record_project_partition_key(&record(Some("project-a")), &identity).unwrap(),
        "project:project-a"
    );
    for project in [
        None,
        Some("project-b"),
        Some(""),
        Some("project:"),
        Some("p\n1"),
    ] {
        assert!(record_project_partition_key(&record(project), &identity).is_err());
    }
    let invalid = SaveRecordInput::user_message("task-1", "message")
        .with_metadata(json!({"memory_project_id":42}));
    assert!(record_project_partition_key(&invalid, &identity).is_err());
}
