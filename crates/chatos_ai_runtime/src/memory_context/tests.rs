// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::Duration,
};

use memory_engine_sdk::{
    ComposeContextBlock, ComposeContextMeta, ComposeContextPolicy, ComposeContextResponse,
    EngineRecord, RuntimeContextAssertionIssuer,
};
use serde_json::{json, Value};

use super::{
    compose_response_to_input_items, compose_response_to_input_items_with_budget,
    standard_compose_policy, MemoryContextCache, MemoryContextComposer, MemoryRecordScope,
    MemoryScope,
};
use crate::tool_runtime::ToolResultModelBudgetLimits;
use crate::{MemoryRecordWriter, SaveRecordInput};

fn test_assertion_issuer() -> RuntimeContextAssertionIssuer {
    RuntimeContextAssertionIssuer::new(
        "runtime-context-test-secret",
        "runtime-context-test-key",
        "runtime-context-test-issuer",
        "runtime-identity-test-v1",
        60,
    )
    .expect("test assertion issuer")
}

#[derive(Clone, Default)]
struct MemoryWriteServerState {
    thread_upserts: Arc<AtomicUsize>,
    record_syncs: Arc<AtomicUsize>,
}

async fn accept_thread_upsert(
    axum::extract::State(state): axum::extract::State<MemoryWriteServerState>,
    axum::extract::Path(thread_id): axum::extract::Path<String>,
    axum::Json(request): axum::Json<Value>,
) -> axum::Json<Value> {
    state.thread_upserts.fetch_add(1, Ordering::SeqCst);
    assert_eq!(request["tenant_id"], "user-1");
    assert_eq!(request["source_id"], "chatos");
    assert_eq!(request["subject_id"], "agent:agent-1");
    assert_eq!(
        request["labels"],
        json!([
            "memory-schema:v2",
            "agent:agent-1",
            "thread-kind:agent_conversation"
        ])
    );
    axum::Json(json!({
        "id": thread_id,
        "tenant_id": "user-1",
        "source_id": "chatos",
        "subject_id": "agent:agent-1",
        "thread_type": "conversation",
        "external_thread_id": "conversation-1",
        "title": null,
        "labels": ["memory-schema:v2", "agent:agent-1", "thread-kind:agent_conversation"],
        "metadata": {"agent_id":"agent-1","agent_name":null,"project_aggregation_id":null,"thread_kind":"agent_conversation"},
        "status": "active",
        "summary_status": "idle",
        "summary_job_run_id": null,
        "summary_locked_at": null,
        "summary_lock_expires_at": null,
        "pending_record_count": 0,
        "pending_summary_tokens": 0,
        "created_at": "2026-10-07T00:00:00Z",
        "updated_at": "2026-10-07T00:00:00Z",
        "archived_at": null
    }))
}

async fn accept_record_sync(
    axum::extract::State(state): axum::extract::State<MemoryWriteServerState>,
    axum::extract::Path(thread_id): axum::extract::Path<String>,
    axum::Json(request): axum::Json<Value>,
) -> axum::Json<Value> {
    assert!(state.thread_upserts.load(Ordering::SeqCst) > 0);
    state.record_syncs.fetch_add(1, Ordering::SeqCst);
    assert_eq!(request["tenant_id"], "user-1");
    assert_eq!(request["source_id"], "chatos");
    axum::Json(json!({
        "thread_id": thread_id,
        "received_count": 1,
        "upserted_count": 1
    }))
}

#[test]
fn memory_scope_builder_keeps_runtime_source_key() {
    let policy = ComposeContextPolicy {
        max_memory_tokens: 20_000,
        max_recent_record_tokens: 12_000,
        max_thread_summary_tokens: 4_000,
        max_project_memory_tokens: 2_000,
        max_agent_memory_tokens: 2_000,
    };
    let scope = MemoryScope::thread("tenant_1", "task_execution", "task_thread_1")
        .with_subject_id("agent:agent_1")
        .with_context_project_subject_id("project:project_1")
        .with_policy(policy);

    assert_eq!(scope.tenant_id, "tenant_1");
    assert_eq!(scope.source_id, "task_execution");
    assert_eq!(scope.thread_id, "task_thread_1");
    assert_eq!(scope.subject_id.as_deref(), Some("agent:agent_1"));
    assert_eq!(
        scope.context_project_subject_id.as_deref(),
        Some("project:project_1")
    );
    assert_eq!(
        scope
            .policy
            .as_ref()
            .map(|value| value.max_recent_record_tokens),
        Some(12_000)
    );
}

#[test]
fn standard_compose_policy_has_a_bounded_raw_tail() {
    let policy = standard_compose_policy();

    assert_eq!(policy.max_memory_tokens, 24_000);
    assert_eq!(policy.max_recent_record_tokens, 12_000);
}

#[test]
fn memory_record_scope_builder_defaults_to_pending_message_records() {
    let scope = MemoryRecordScope::new("tenant_1")
        .with_thread_id("thread_1")
        .with_record_type("task_message")
        .with_default_summary_status(None);

    assert_eq!(scope.tenant_id, "tenant_1");
    assert_eq!(scope.thread_id.as_deref(), Some("thread_1"));
    assert_eq!(scope.record_type, "task_message");
    assert!(scope.default_summary_status.is_none());

    let message_scope = MemoryRecordScope::message_thread("tenant_1", "thread_2");
    assert_eq!(message_scope.record_type, "message");
    assert_eq!(
        message_scope.default_summary_status.as_deref(),
        Some("pending")
    );

    let routed_scope = MemoryRecordScope::per_record_tenant("tenant_id");
    assert!(routed_scope.tenant_id.is_empty());
    assert_eq!(
        routed_scope.tenant_metadata_key.as_deref(),
        Some("tenant_id")
    );
}

#[test]
fn memory_record_writer_resolves_multi_user_tenant_from_record_metadata() {
    let client = memory_engine_sdk::MemoryEngineClient::new_direct(
        "http://127.0.0.1:1",
        Duration::from_secs(1),
        "local_agent",
    )
    .expect("client");
    let writer = super::MemoryEngineRecordWriter::from_client(
        client,
        MemoryRecordScope::per_record_tenant("tenant_id"),
    )
    .with_runtime_context_assertion_issuer(test_assertion_issuer());
    let record = SaveRecordInput::user_message("conversation-1", "hello")
        .with_metadata(json!({"tenant_id": "user-1"}));

    assert_eq!(
        writer.tenant_id_for_record(&record).expect("tenant"),
        "user-1"
    );
    let missing = SaveRecordInput::user_message("conversation-2", "hello");
    assert!(writer.tenant_id_for_record(&missing).is_err());
}

#[tokio::test]
async fn memory_record_writer_creates_each_thread_before_syncing_records() {
    use axum::{routing::put, Router};

    let state = MemoryWriteServerState::default();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener");
    let address = listener.local_addr().expect("address");
    let server = tokio::spawn({
        let state = state.clone();
        async move {
            axum::serve(
                listener,
                Router::new()
                    .route(
                        "/api/memory-engine/v1/threads/{thread_id}",
                        put(accept_thread_upsert),
                    )
                    .route(
                        "/api/memory-engine/v1/threads/{thread_id}/records/batch-sync",
                        put(accept_record_sync),
                    )
                    .with_state(state),
            )
            .await
            .expect("server");
        }
    });
    let client = memory_engine_sdk::MemoryEngineClient::new_direct(
        format!("http://{address}"),
        Duration::from_secs(1),
        "chatos",
    )
    .expect("client");
    let writer = super::MemoryEngineRecordWriter::from_client(
        client,
        MemoryRecordScope::per_record_tenant("tenant_id"),
    )
    .with_runtime_context_assertion_issuer(test_assertion_issuer());
    writer
        .save_record(
            SaveRecordInput::user_message("conversation-1", "hello")
                .with_message_id("message-1")
                .with_message_mode("agent_conversation")
                .with_metadata(json!({
                    "tenant_id": "user-1",
                    "agent_id": "agent-1"
                })),
        )
        .await
        .expect("top-level message mode establishes the thread kind");
    writer
        .save_record(
            SaveRecordInput::user_message("conversation-1", "continue")
                .with_message_id("message-2")
                .with_metadata(json!({
                    "tenant_id": "user-1",
                    "agent_id": "agent-1",
                    "thread_kind": "agent_conversation"
                })),
        )
        .await
        .expect("explicit thread kind matches the established message mode");

    assert_eq!(state.thread_upserts.load(Ordering::SeqCst), 1);
    assert_eq!(state.record_syncs.load(Ordering::SeqCst), 2);
    let identity_change = writer
        .save_record(
            SaveRecordInput::user_message("conversation-1", "wrong Agent")
                .with_message_id("message-3")
                .with_metadata(json!({
                    "tenant_id": "user-1",
                    "agent_id": "agent-2",
                    "thread_kind": "agent_conversation"
                })),
        )
        .await;
    assert_eq!(
        identity_change.expect_err("thread identity change must fail"),
        "memory thread identity changed after it was established"
    );
    assert_eq!(state.record_syncs.load(Ordering::SeqCst), 2);
    server.abort();
}

#[tokio::test]
async fn composer_establishes_a_new_agent_thread_before_first_recall() {
    use axum::{routing::put, Router};

    let state = MemoryWriteServerState::default();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener");
    let address = listener.local_addr().expect("address");
    let server = tokio::spawn({
        let state = state.clone();
        async move {
            axum::serve(
                listener,
                Router::new()
                    .route(
                        "/api/memory-engine/v1/threads/{thread_id}",
                        put(accept_thread_upsert),
                    )
                    .with_state(state),
            )
            .await
            .expect("server");
        }
    });
    let composer = MemoryContextComposer::new_direct(
        format!("http://{address}"),
        Duration::from_secs(1),
        "chatos",
    )
    .expect("composer");
    let scope = MemoryScope::thread("user-1", "chatos", "conversation-1")
        .with_subject_id("agent:agent-1")
        .with_context_project_subject_id("project:project-1");
    let record =
        SaveRecordInput::user_message("conversation-1", "first message").with_metadata(json!({
            "tenant_id": "user-1",
            "agent_id": "agent-1",
            "thread_kind": "agent_conversation",
            "thread_project_aggregation_id": null,
        }));

    composer
        .ensure_thread_for_record(&scope, &record)
        .await
        .expect("ensure first Agent thread");

    assert_eq!(state.thread_upserts.load(Ordering::SeqCst), 1);
    server.abort();
}

#[test]
fn direct_composer_rejects_mismatched_scope_source_key() {
    let composer =
        MemoryContextComposer::new_direct("http://127.0.0.1:1", Duration::from_secs(1), "chatos")
            .expect("composer");
    assert_eq!(composer.source_id(), Some("chatos"));

    let matching = MemoryScope::thread("tenant_1", "chatos", "thread_1");
    composer
        .validate_scope_source(&matching)
        .expect("matching scope source");

    let mismatched = MemoryScope::thread("tenant_1", "task_execution", "thread_1");
    let err = composer
        .validate_scope_source(&mismatched)
        .expect_err("mismatched scope source");
    assert!(err.contains("source_id mismatch"));
}

struct TestContextCache;

#[async_trait::async_trait]
impl MemoryContextCache for TestContextCache {
    async fn load(&self, _scope: &MemoryScope) -> Result<Option<ComposeContextResponse>, String> {
        panic!("compose failures must not read stale cached context")
    }

    async fn store(
        &self,
        _scope: &MemoryScope,
        _response: &ComposeContextResponse,
    ) -> Result<(), String> {
        Ok(())
    }
}

#[tokio::test]
async fn composer_fails_closed_when_remote_is_offline() {
    use axum::{http::StatusCode, Router};

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener");
    let address = listener.local_addr().expect("address");
    let server = tokio::spawn(async move {
        axum::serve(
            listener,
            Router::new().fallback(|| async {
                (
                    StatusCode::SERVICE_UNAVAILABLE,
                    "Memory temporarily offline",
                )
            }),
        )
        .await
        .expect("server");
    });
    let scope = MemoryScope::thread("user-1", "local_agent", "conversation-1")
        .with_subject_id("agent:test-agent")
        .with_compose_context(None, None, "capability-v1");
    let composer = MemoryContextComposer::new_direct(
        format!("http://{address}"),
        Duration::from_secs(1),
        "local_agent",
    )
    .expect("composer")
    .with_runtime_context_assertion_issuer(test_assertion_issuer())
    .with_resilient_cache(TestContextCache);
    assert!(composer.compose(&scope).await.is_err());
    server.abort();
}

#[test]
fn compose_response_to_input_items_rebuilds_tool_exchange_in_responses_shape() {
    let response = ComposeContextResponse {
        thread_id: "thread-1".to_string(),
        blocks: vec![ComposeContextBlock {
            block_type: "summary".to_string(),
            text: "recent summary".to_string(),
            ..Default::default()
        }],
        recent_records: vec![
            EngineRecord {
                id: "rec-1".to_string(),
                thread_id: "thread-1".to_string(),
                tenant_id: "tenant-1".to_string(),
                source_id: "task".to_string(),
                external_record_id: None,
                role: "assistant".to_string(),
                record_type: "message".to_string(),
                content: "calling tool".to_string(),
                structured_payload: None,
                metadata: Some(json!({
                    "tool_calls": [{
                        "id": "call_1",
                        "type": "function",
                        "function": {
                            "name": "demo.search",
                            "arguments": "{\"q\":\"rust\"}"
                        }
                    }]
                })),
                summary_status: "pending".to_string(),
                summary_id: None,
                summarized_at: None,
                created_at: "2026-06-08T00:00:00Z".to_string(),
            },
            EngineRecord {
                id: "rec-2".to_string(),
                thread_id: "thread-1".to_string(),
                tenant_id: "tenant-1".to_string(),
                source_id: "task".to_string(),
                external_record_id: None,
                role: "tool".to_string(),
                record_type: "message".to_string(),
                content: "done".to_string(),
                structured_payload: None,
                metadata: Some(json!({
                    "tool_call_id": "call_1"
                })),
                summary_status: "pending".to_string(),
                summary_id: None,
                summarized_at: None,
                created_at: "2026-06-08T00:00:01Z".to_string(),
            },
        ],
        meta: ComposeContextMeta {
            summary_count: 1,
            recent_record_count: 2,
            ..Default::default()
        },
    };

    let items = compose_response_to_input_items(&response);
    assert_eq!(
        items[0].get("type").and_then(|value| value.as_str()),
        Some("message")
    );
    assert!(items.iter().any(|item| {
        item.get("type").and_then(|value| value.as_str()) == Some("function_call")
            && item.get("call_id").and_then(|value| value.as_str()) == Some("call_1")
    }));
    assert!(items.iter().any(|item| {
        item.get("type").and_then(|value| value.as_str()) == Some("function_call_output")
            && item.get("call_id").and_then(|value| value.as_str()) == Some("call_1")
    }));
    assert!(!items
        .iter()
        .any(|item| { item.get("role").and_then(|value| value.as_str()) == Some("tool") }));
}

#[test]
fn compose_response_to_input_items_skips_orphan_tool_outputs() {
    let response = ComposeContextResponse {
        thread_id: "thread-1".to_string(),
        blocks: Vec::new(),
        recent_records: vec![EngineRecord {
            id: "rec-1".to_string(),
            thread_id: "thread-1".to_string(),
            tenant_id: "tenant-1".to_string(),
            source_id: "task".to_string(),
            external_record_id: None,
            role: "tool".to_string(),
            record_type: "message".to_string(),
            content: "done".to_string(),
            structured_payload: None,
            metadata: Some(json!({
                "tool_call_id": "call_missing"
            })),
            summary_status: "pending".to_string(),
            summary_id: None,
            summarized_at: None,
            created_at: "2026-06-08T00:00:01Z".to_string(),
        }],
        meta: ComposeContextMeta {
            summary_count: 0,
            recent_record_count: 1,
            ..Default::default()
        },
    };

    let items = compose_response_to_input_items(&response);
    assert!(items.is_empty());
}

#[test]
fn compose_response_to_input_items_skips_orphan_tool_calls() {
    let response = ComposeContextResponse {
        thread_id: "thread-1".to_string(),
        blocks: Vec::new(),
        recent_records: vec![EngineRecord {
            id: "rec-1".to_string(),
            thread_id: "thread-1".to_string(),
            tenant_id: "tenant-1".to_string(),
            source_id: "task".to_string(),
            external_record_id: None,
            role: "assistant".to_string(),
            record_type: "message".to_string(),
            content: "calling tool".to_string(),
            structured_payload: None,
            metadata: Some(json!({
                "tool_calls": [{
                    "id": "call_1",
                    "type": "function",
                    "function": {
                        "name": "demo.search",
                        "arguments": "{\"q\":\"rust\"}"
                    }
                }]
            })),
            summary_status: "pending".to_string(),
            summary_id: None,
            summarized_at: None,
            created_at: "2026-06-08T00:00:00Z".to_string(),
        }],
        meta: ComposeContextMeta {
            summary_count: 0,
            recent_record_count: 1,
            ..Default::default()
        },
    };

    let items = compose_response_to_input_items(&response);

    assert!(items.iter().any(|item| {
        item.get("type").and_then(|value| value.as_str()) == Some("message")
            && item.get("role").and_then(|value| value.as_str()) == Some("assistant")
    }));
    assert!(!items.iter().any(|item| {
        item.get("type").and_then(|value| value.as_str()) == Some("function_call")
    }));
}

#[test]
fn compose_response_to_input_items_rejects_unpaged_overflow_without_damaged_excerpt() {
    let response = ComposeContextResponse {
        thread_id: "thread-1".to_string(),
        blocks: Vec::new(),
        recent_records: vec![
            EngineRecord {
                id: "rec-1".to_string(),
                thread_id: "thread-1".to_string(),
                tenant_id: "tenant-1".to_string(),
                source_id: "task".to_string(),
                external_record_id: None,
                role: "assistant".to_string(),
                record_type: "message".to_string(),
                content: "calling tool".to_string(),
                structured_payload: None,
                metadata: Some(json!({
                    "tool_calls": [{
                        "id": "call_1",
                        "type": "function",
                        "function": {
                            "name": "code.read_file",
                            "arguments": "{\"path\":\"big.log\"}"
                        }
                    }]
                })),
                summary_status: "pending".to_string(),
                summary_id: None,
                summarized_at: None,
                created_at: "2026-06-08T00:00:00Z".to_string(),
            },
            EngineRecord {
                id: "rec-2".to_string(),
                thread_id: "thread-1".to_string(),
                tenant_id: "tenant-1".to_string(),
                source_id: "task".to_string(),
                external_record_id: None,
                role: "tool".to_string(),
                record_type: "message".to_string(),
                content: "x".repeat(160_001),
                structured_payload: None,
                metadata: Some(json!({
                    "tool_call_id": "call_1",
                    "name": "code.read_file"
                })),
                summary_status: "pending".to_string(),
                summary_id: None,
                summarized_at: None,
                created_at: "2026-06-08T00:00:01Z".to_string(),
            },
        ],
        meta: ComposeContextMeta {
            summary_count: 0,
            recent_record_count: 2,
            ..Default::default()
        },
    };

    let items = compose_response_to_input_items(&response);
    let output = items
        .iter()
        .find(|item| {
            item.get("type").and_then(|value| value.as_str()) == Some("function_call_output")
        })
        .and_then(|item| item.get("output"))
        .and_then(|value| value.as_str())
        .unwrap_or_default();

    let diagnostic: Value = serde_json::from_str(output).expect("valid overflow diagnostic");
    assert_eq!(diagnostic["error"], "tool_result_budget_exceeded");
    assert_eq!(diagnostic["tool"], "code.read_file");
    assert_eq!(diagnostic["original_chars"], 160_001);
    assert_eq!(diagnostic["content_preserved"], false);
    assert!(!output.contains("tool_result_truncated"));
    assert!(!output.starts_with('x'));
}

#[test]
fn compose_response_to_input_items_keeps_oldest_output_and_rejects_later_overflow() {
    let tool_call_record = |record_id: &str, call_id: &str, created_at: &str| EngineRecord {
        id: record_id.to_string(),
        thread_id: "thread-1".to_string(),
        tenant_id: "tenant-1".to_string(),
        source_id: "task".to_string(),
        external_record_id: None,
        role: "assistant".to_string(),
        record_type: "message".to_string(),
        content: "calling tool".to_string(),
        structured_payload: None,
        metadata: Some(json!({
            "tool_calls": [{
                "id": call_id,
                "type": "function",
                "function": {"name": "task_execution_list_tasks", "arguments": "{}"}
            }]
        })),
        summary_status: "pending".to_string(),
        summary_id: None,
        summarized_at: None,
        created_at: created_at.to_string(),
    };
    let tool_output_record =
        |record_id: &str, call_id: &str, content: &str, created_at: &str| EngineRecord {
            id: record_id.to_string(),
            thread_id: "thread-1".to_string(),
            tenant_id: "tenant-1".to_string(),
            source_id: "task".to_string(),
            external_record_id: None,
            role: "tool".to_string(),
            record_type: "message".to_string(),
            content: content.to_string(),
            structured_payload: None,
            metadata: Some(json!({
                "tool_call_id": call_id,
                "name": "task_execution_list_tasks"
            })),
            summary_status: "pending".to_string(),
            summary_id: None,
            summarized_at: None,
            created_at: created_at.to_string(),
        };
    let response = ComposeContextResponse {
        thread_id: "thread-1".to_string(),
        blocks: Vec::new(),
        recent_records: vec![
            tool_call_record("rec-1", "call_old", "2026-06-08T00:00:00Z"),
            tool_output_record(
                "rec-2",
                "call_old",
                "{\"count\":3,\"tasks\":[\"older-task-state\"]}",
                "2026-06-08T00:00:01Z",
            ),
            tool_call_record("rec-3", "call_new", "2026-06-08T00:00:02Z"),
            tool_output_record(
                "rec-4",
                "call_new",
                "{\"count\":0,\"tasks\":[]}",
                "2026-06-08T00:00:03Z",
            ),
        ],
        meta: ComposeContextMeta {
            summary_count: 0,
            recent_record_count: 4,
            ..Default::default()
        },
    };

    let items = compose_response_to_input_items_with_budget(
        &response,
        Some(ToolResultModelBudgetLimits::new(100, 45)),
    );
    let outputs = items
        .iter()
        .filter_map(|item| {
            let call_id = item.get("call_id").and_then(|value| value.as_str())?;
            let output = item.get("output").and_then(|value| value.as_str())?;
            Some((call_id, output))
        })
        .collect::<HashMap<_, _>>();

    assert_eq!(
        outputs.get("call_old").copied(),
        Some("{\"count\":3,\"tasks\":[\"older-task-state\"]}")
    );
    let diagnostic: Value = serde_json::from_str(outputs["call_new"]).expect("overflow JSON");
    assert_eq!(diagnostic["error"], "tool_result_budget_exceeded");
    assert_eq!(diagnostic["reason"], "total_budget");
    assert_eq!(diagnostic["available_chars"], 5);
    assert_eq!(diagnostic["content_preserved"], false);
    assert!(!outputs["call_new"].starts_with("{\"cou"));
}

#[test]
fn compose_response_to_input_items_reads_tool_calls_from_structured_payload() {
    let response = ComposeContextResponse {
        thread_id: "thread-1".to_string(),
        blocks: Vec::new(),
        recent_records: vec![
            EngineRecord {
                id: "rec-1".to_string(),
                thread_id: "thread-1".to_string(),
                tenant_id: "tenant-1".to_string(),
                source_id: "task".to_string(),
                external_record_id: None,
                role: "assistant".to_string(),
                record_type: "message".to_string(),
                content: "calling tool".to_string(),
                structured_payload: Some(json!([{
                    "id": "call_1",
                    "type": "function",
                    "function": {
                        "name": "demo.search",
                        "arguments": "{\"q\":\"rust\"}"
                    }
                }])),
                metadata: None,
                summary_status: "pending".to_string(),
                summary_id: None,
                summarized_at: None,
                created_at: "2026-06-08T00:00:00Z".to_string(),
            },
            EngineRecord {
                id: "rec-2".to_string(),
                thread_id: "thread-1".to_string(),
                tenant_id: "tenant-1".to_string(),
                source_id: "task".to_string(),
                external_record_id: None,
                role: "tool".to_string(),
                record_type: "message".to_string(),
                content: "done".to_string(),
                structured_payload: None,
                metadata: Some(json!({
                    "tool_call_id": "call_1"
                })),
                summary_status: "pending".to_string(),
                summary_id: None,
                summarized_at: None,
                created_at: "2026-06-08T00:00:01Z".to_string(),
            },
        ],
        meta: ComposeContextMeta {
            summary_count: 0,
            recent_record_count: 2,
            ..Default::default()
        },
    };

    let items = compose_response_to_input_items(&response);
    assert!(items.iter().any(|item| {
        item.get("type").and_then(|value| value.as_str()) == Some("function_call")
            && item.get("call_id").and_then(|value| value.as_str()) == Some("call_1")
    }));
    assert!(items.iter().any(|item| {
        item.get("type").and_then(|value| value.as_str()) == Some("function_call_output")
            && item.get("call_id").and_then(|value| value.as_str()) == Some("call_1")
    }));
}
