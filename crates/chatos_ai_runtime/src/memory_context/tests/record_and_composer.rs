// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

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
fn memory_record_scope_routes_tenant_from_record_metadata() {
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
    .with_runtime_context_assertion_provider(test_assertion_issuer());
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
    .with_runtime_context_assertion_provider(test_assertion_issuer());
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

struct TestContextCache(Option<ComposeContextResponse>);

#[async_trait::async_trait]
impl MemoryContextCache for TestContextCache {
    async fn load(&self, _scope: &MemoryScope) -> Result<Option<ComposeContextResponse>, String> {
        Ok(self.0.clone())
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
async fn composer_uses_fresh_cache_when_remote_is_temporarily_unavailable() {
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
        .with_policy(ComposeContextPolicy {
            max_memory_tokens: 1_000,
            max_recent_record_tokens: 600,
            max_thread_summary_tokens: 200,
            max_project_memory_tokens: 100,
            max_agent_memory_tokens: 100,
        })
        .with_compose_context(None, None, "capability-v1");
    let cached = ComposeContextResponse {
        thread_id: scope.thread_id.clone(),
        blocks: vec![ComposeContextBlock {
            block_type: "thread_summary".to_string(),
            text: "cached context".to_string(),
            ..Default::default()
        }],
        recent_records: Vec::new(),
        meta: ComposeContextMeta::default(),
    };
    let composer = MemoryContextComposer::new_direct(
        format!("http://{address}"),
        Duration::from_secs(1),
        "local_agent",
    )
    .expect("composer")
    .with_runtime_context_assertion_provider(test_assertion_issuer())
    .with_resilient_cache(TestContextCache(Some(cached)));
    let response = composer.compose(&scope).await.expect("cached fallback");
    assert_eq!(response.blocks[0].text, "cached context");

    let composer = MemoryContextComposer::new_direct(
        format!("http://{address}"),
        Duration::from_secs(1),
        "local_agent",
    )
    .expect("composer")
    .with_runtime_context_assertion_provider(test_assertion_issuer())
    .with_resilient_cache(TestContextCache(None));
    let response = composer.compose(&scope).await.expect("empty fallback");
    assert!(response.blocks.is_empty());
    assert!(response.recent_records.is_empty());
    server.abort();
}

#[test]
fn only_transient_memory_failures_are_degraded() {
    assert!(super::is_transient_memory_error(
        "stage=decode_json is_timeout=true is_connect=false"
    ));
    assert!(super::is_transient_memory_error(
        "status=503 Service Unavailable"
    ));
    assert!(!super::is_transient_memory_error(
        "status=400 Bad Request detail=invalid scope"
    ));
}

#[tokio::test]
async fn composer_returns_budgeted_context_without_waiting_for_background_compaction() {
    use axum::{routing::post, Router};

    let state = BackgroundCompactionState::default();
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
                        "/api/memory-engine/v1/context/compose",
                        post(compose_budgeted_context),
                    )
                    .route(
                        "/api/memory-engine/v1/threads/{thread_id}/active-summary/run",
                        post(accept_background_compaction),
                    )
                    .with_state(state),
            )
            .await
            .expect("server");
        }
    });
    let composer = MemoryContextComposer::new_direct(
        format!("http://{address}"),
        Duration::from_secs(2),
        "local_agent",
    )
    .expect("composer")
    .with_runtime_context_assertion_provider(test_assertion_issuer());
    let scope = MemoryScope::thread("user-1", "local_agent", "conversation-1")
        .with_subject_id("agent:test-agent")
        .with_policy(ComposeContextPolicy {
            max_memory_tokens: 1_000,
            max_recent_record_tokens: 600,
            max_thread_summary_tokens: 200,
            max_project_memory_tokens: 100,
            max_agent_memory_tokens: 100,
        })
        .with_compose_context(None, None, "capability-v1");

    let response = composer.compose(&scope).await.expect("compose");
    assert_eq!(response.blocks[0].text, "budgeted history");
    tokio::time::timeout(Duration::from_secs(1), async {
        while state.triggers.load(Ordering::SeqCst) == 0 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("background compaction trigger");
    state.release.notify_one();
    server.abort();
}
