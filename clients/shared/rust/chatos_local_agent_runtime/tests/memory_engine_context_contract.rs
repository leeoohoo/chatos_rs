// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::{collections::VecDeque, sync::Arc, time::Duration};

use async_trait::async_trait;
use chatos_local_agent_runtime::{
    ActiveSummaryWaitPolicy, MemoryEngineContextAdapter, MemoryEngineContextApi,
    MemoryEngineContextError, MemoryEngineContextScope,
};
use chatos_memory_client::{
    ComposeContextBlock, ComposeContextMeta, ComposeContextRequest, ComposeContextResponse,
    EngineRecord, RunThreadActiveSummaryResponse,
};
use serde_json::{json, Value};
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

#[derive(Default)]
struct MockMemoryApi {
    calls: Mutex<Vec<String>>,
    statuses: Mutex<VecDeque<RunThreadActiveSummaryResponse>>,
    run_status: Mutex<Option<RunThreadActiveSummaryResponse>>,
    compose_response: Mutex<Option<ComposeContextResponse>>,
}

#[async_trait]
impl MemoryEngineContextApi for MockMemoryApi {
    async fn compose_context(
        &self,
        request: &ComposeContextRequest,
    ) -> Result<ComposeContextResponse, String> {
        self.calls
            .lock()
            .await
            .push(format!("compose:{}", request.thread_id));
        self.compose_response
            .lock()
            .await
            .clone()
            .ok_or_else(|| "missing compose response".to_string())
    }

    async fn run_active_summary(
        &self,
        thread_id: &str,
        _tenant_id: &str,
        trigger_reason: Option<&str>,
    ) -> Result<RunThreadActiveSummaryResponse, String> {
        self.calls.lock().await.push(format!(
            "run:{thread_id}:{}",
            trigger_reason.unwrap_or_default()
        ));
        self.run_status
            .lock()
            .await
            .clone()
            .ok_or_else(|| "missing run status".to_string())
    }

    async fn get_active_summary_status(
        &self,
        thread_id: &str,
        _tenant_id: &str,
        job_run_id: Option<&str>,
    ) -> Result<RunThreadActiveSummaryResponse, String> {
        self.calls.lock().await.push(format!(
            "status:{thread_id}:{}",
            job_run_id.unwrap_or_default()
        ));
        self.statuses
            .lock()
            .await
            .pop_front()
            .ok_or_else(|| "missing status".to_string())
    }
}

#[tokio::test]
async fn prepare_waits_for_existing_summary_then_composes_authoritative_context() {
    let api = Arc::new(MockMemoryApi::default());
    api.statuses.lock().await.extend([
        summary_status(true, false, false, Some("job-1")),
        summary_status(false, true, false, Some("job-1")),
    ]);
    *api.compose_response.lock().await = Some(valid_compose_response());
    let adapter = adapter(api.clone());

    let prepared = adapter
        .prepare_model_input(&scope(), &CancellationToken::new())
        .await
        .expect("prepared context");

    assert_eq!(prepared.summary_count, 1);
    assert_eq!(prepared.recent_record_count, 3);
    assert!(prepared.input_items.iter().any(|item| {
        item.get("type").and_then(Value::as_str) == Some("function_call")
            && item.get("call_id").and_then(Value::as_str) == Some("call-1")
    }));
    assert!(prepared.input_items.iter().any(|item| {
        item.get("type").and_then(Value::as_str) == Some("function_call_output")
            && item.get("call_id").and_then(Value::as_str) == Some("call-1")
    }));
    assert_eq!(
        *api.calls.lock().await,
        vec![
            "status:thread-1:".to_string(),
            "status:thread-1:job-1".to_string(),
            "compose:thread-1".to_string(),
        ]
    );
}

#[tokio::test]
async fn requested_summary_recomposes_only_after_formal_completion() {
    let api = Arc::new(MockMemoryApi::default());
    *api.run_status.lock().await = Some(summary_status(true, false, false, Some("job-2")));
    api.statuses
        .lock()
        .await
        .push_back(summary_status(false, true, false, Some("job-2")));
    *api.compose_response.lock().await = Some(valid_compose_response());
    let adapter = adapter(api.clone());

    adapter
        .summarize_and_prepare(
            &scope(),
            Some("active_token_threshold"),
            &CancellationToken::new(),
        )
        .await
        .expect("summarized context");

    assert_eq!(
        *api.calls.lock().await,
        vec![
            "run:thread-1:active_token_threshold".to_string(),
            "status:thread-1:job-2".to_string(),
            "compose:thread-1".to_string(),
        ]
    );
}

#[tokio::test]
async fn cancellation_interrupts_summary_wait_without_composing() {
    let api = Arc::new(MockMemoryApi::default());
    api.statuses
        .lock()
        .await
        .push_back(summary_status(true, false, false, Some("job-3")));
    let adapter = MemoryEngineContextAdapter::new(api.clone(), "source-1")
        .unwrap()
        .with_wait_policy(ActiveSummaryWaitPolicy {
            poll_interval: Duration::from_secs(30),
            maximum_wait: Duration::from_secs(15 * 60),
        })
        .unwrap();
    let cancellation = CancellationToken::new();
    cancellation.cancel();

    let error = adapter
        .prepare_model_input(&scope(), &cancellation)
        .await
        .expect_err("cancelled");

    assert_eq!(error, MemoryEngineContextError::Cancelled);
    assert!(api.calls.lock().await.is_empty());
}

#[tokio::test]
async fn cross_scope_compose_data_fails_closed() {
    let api = Arc::new(MockMemoryApi::default());
    api.statuses
        .lock()
        .await
        .push_back(summary_status(false, false, false, None));
    let mut response = valid_compose_response();
    response.recent_records[0].tenant_id = "another-tenant".to_string();
    *api.compose_response.lock().await = Some(response);

    let error = adapter(api)
        .prepare_model_input(&scope(), &CancellationToken::new())
        .await
        .expect_err("cross-scope response");

    assert!(matches!(
        error,
        MemoryEngineContextError::InvalidComposeResponse { .. }
    ));
}

#[tokio::test]
async fn failed_summary_is_not_treated_as_compacted_context() {
    let api = Arc::new(MockMemoryApi::default());
    let mut failed = summary_status(false, true, true, Some("job-4"));
    failed.error_message = Some("summary model failed".to_string());
    api.statuses.lock().await.push_back(failed);

    let error = adapter(api)
        .prepare_model_input(&scope(), &CancellationToken::new())
        .await
        .expect_err("summary failure");

    assert_eq!(
        error,
        MemoryEngineContextError::SummaryFailed {
            detail: "summary model failed".to_string()
        }
    );
}

fn adapter(api: Arc<MockMemoryApi>) -> MemoryEngineContextAdapter {
    MemoryEngineContextAdapter::new(api, "source-1")
        .unwrap()
        .with_wait_policy(ActiveSummaryWaitPolicy {
            poll_interval: Duration::from_millis(1),
            maximum_wait: Duration::from_secs(1),
        })
        .unwrap()
}

fn scope() -> MemoryEngineContextScope {
    MemoryEngineContextScope::thread("tenant-1", "source-1", "thread-1").unwrap()
}

fn summary_status(
    running: bool,
    completed: bool,
    failed: bool,
    job_run_id: Option<&str>,
) -> RunThreadActiveSummaryResponse {
    RunThreadActiveSummaryResponse {
        thread_id: "thread-1".to_string(),
        accepted: running || completed,
        running,
        completed,
        failed,
        job_run_id: job_run_id.map(ToOwned::to_owned),
        generated: completed,
        summary_id: completed.then(|| "summary-1".to_string()),
        source_record_count: 3,
        pending_before_count: Some(3),
        pending_after_count: completed.then_some(0),
        compacted: completed,
        error_message: None,
    }
}

fn valid_compose_response() -> ComposeContextResponse {
    ComposeContextResponse {
        thread_id: "thread-1".to_string(),
        blocks: vec![ComposeContextBlock {
            block_type: "thread_summary".to_string(),
            text: "The user is building a local agent runtime.".to_string(),
        }],
        recent_records: vec![
            record(
                "record-1",
                "assistant",
                "I will inspect the repository.",
                Some(json!({
                    "tool_calls": [{
                        "id": "call-1",
                        "function": {"name": "repo.inspect", "arguments": "{}"}
                    }]
                })),
                None,
            ),
            record(
                "record-2",
                "tool",
                "inspection complete",
                None,
                Some(json!({"tool_call_id": "call-1"})),
            ),
            record("record-3", "user", "Continue.", None, None),
        ],
        meta: ComposeContextMeta {
            summary_count: 1,
            recent_record_count: 3,
        },
    }
}

fn record(
    id: &str,
    role: &str,
    content: &str,
    structured_payload: Option<Value>,
    metadata: Option<Value>,
) -> EngineRecord {
    EngineRecord {
        id: id.to_string(),
        thread_id: "thread-1".to_string(),
        tenant_id: "tenant-1".to_string(),
        source_id: "source-1".to_string(),
        external_record_id: None,
        role: role.to_string(),
        record_type: "message".to_string(),
        content: content.to_string(),
        structured_payload,
        metadata,
        summary_status: "pending".to_string(),
        summary_id: None,
        summarized_at: None,
        created_at: "2026-09-12T00:00:00Z".to_string(),
    }
}
