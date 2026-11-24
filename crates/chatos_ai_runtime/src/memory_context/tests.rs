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
    MemoryContextCache, MemoryContextComposer, MemoryRecordScope, MemoryScope,
};
use crate::tool_runtime::ToolResultModelBudgetLimits;
use crate::{MemoryRecordWriter, SaveRecordInput};

pub(super) fn test_assertion_issuer() -> RuntimeContextAssertionIssuer {
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
pub(super) struct MemoryWriteServerState {
    thread_upserts: Arc<AtomicUsize>,
    record_syncs: Arc<AtomicUsize>,
}

#[derive(Clone, Default)]
struct BackgroundCompactionState {
    triggers: Arc<AtomicUsize>,
    release: Arc<tokio::sync::Notify>,
}

async fn compose_budgeted_context() -> axum::Json<ComposeContextResponse> {
    let meta = ComposeContextMeta {
        budget_used: memory_engine_sdk::ComposeTokenBudget {
            total: 10,
            ..Default::default()
        },
        needs_compaction: true,
        ..Default::default()
    };
    axum::Json(ComposeContextResponse {
        thread_id: "conversation-1".to_string(),
        blocks: vec![ComposeContextBlock {
            block_type: "thread_summary".to_string(),
            pool: "thread_continuity".to_string(),
            text: "budgeted history".to_string(),
            estimated_tokens: 10,
            ..Default::default()
        }],
        recent_records: Vec::new(),
        meta,
    })
}

async fn accept_background_compaction(
    axum::extract::State(state): axum::extract::State<BackgroundCompactionState>,
) -> axum::Json<Value> {
    state.triggers.fetch_add(1, Ordering::SeqCst);
    state.release.notified().await;
    axum::Json(json!({
        "thread_id": "conversation-1",
        "accepted": true,
        "running": true,
        "completed": false,
        "failed": false,
        "job_run_id": "job-1",
        "generated": false,
        "summary_id": null,
        "source_record_count": 0,
        "pending_before_count": 10,
        "pending_after_count": null,
        "compacted": false,
        "error_message": null
    }))
}

pub(super) async fn accept_thread_upsert(
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
    assert_eq!(request["thread"]["tenant_id"], "user-1");
    assert_eq!(request["thread"]["subject_id"], "agent:agent-1");
    assert_eq!(request["thread"]["thread_type"], "conversation");
    state.thread_upserts.fetch_add(1, Ordering::SeqCst);
    state.record_syncs.fetch_add(1, Ordering::SeqCst);
    assert_eq!(request["tenant_id"], "user-1");
    assert_eq!(request["source_id"], "chatos");
    axum::Json(json!({
        "thread_id": thread_id,
        "received_count": 1,
        "upserted_count": 1
    }))
}

#[path = "tests/compose_items_basic.rs"]
mod compose_items_basic;
#[path = "tests/compose_items_budget.rs"]
mod compose_items_budget;
#[path = "tests/record_and_composer.rs"]
mod record_and_composer;
